use std::cell::{Ref, RefCell, RefMut};
use std::marker::PhantomData;
use std::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard,
    },
};

/// Every object root is assigned a Tag, which we enforce is globally unique.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Tag(u64);
static NEXT_TAG: AtomicU64 = AtomicU64::new(0);
impl Tag {
    pub fn new() -> Self {
        Self(NEXT_TAG.fetch_add(1, Ordering::Relaxed))
    }
}

/// Tracks which `Root` is locked for the current thread.
struct ThreadRootTracker {
    // Tag of the Root that's currenty locked, if any.
    //
    // We *could* support multiple roots being locked at once, but using e.g.  a
    // hash table here adds significant overhead. Instead we support multiple
    // Root types, each with its own tracker.
    current_tag: Option<Tag>,
    // Force to *not* be Send nor Sync, since it tracks state of the *current
    // thread*.
    // Probably not strictly necessary while this struct is private, since we
    // already only ever store instances in a thread-local.
    _phantom: std::marker::PhantomData<*mut std::ffi::c_void>,
}

impl ThreadRootTracker {
    fn new() -> Self {
        Self {
            current_tag: None,
            _phantom: std::marker::PhantomData,
        }
    }

    fn has_tag(&self, tag: Tag) -> bool {
        self.current_tag == Some(tag)
    }

    fn add_tag(&mut self, tag: Tag) {
        debug_assert!(
            self.current_tag.is_none(),
            "Tried adding tag {:?} with tag {:?} already held",
            tag,
            self.current_tag
        );

        self.current_tag = Some(tag)
    }

    fn clear_tag(&mut self, tag: Tag) {
        debug_assert!(
            self.has_tag(tag),
            "Tried clearing tag {:?} which isn't held",
            tag
        );

        self.current_tag.take();
    }
}

/// Root of an "object graph". It holds a lock over the contents of the graph,
/// and ensures tracks which tags are locked by the current thread.
///
/// We only support a thread having one Root of any given type T being locked at
/// once. Crate users should use a private type T that they own to avoid
/// conflicts.
pub struct Root<T> {
    root: ManuallyDrop<Mutex<T>>,
    tag: Tag,
}

impl<T> Root<T> {
    thread_local! {
        /// Must be unique per thread. Must also be accessible by e.g.
        /// `RootedRc::Drop`, which is easiest if it's in a thread-local.
        ///
        /// Maybe parameterize by tracker type and location as well? A type
        /// would probably be straightforward, though having the user provide an
        /// object is trickier, since again we need to be able to get to the
        /// tracker from `Drop` implementations.
        static THREAD_ROOT_TRACKER : RefCell<ThreadRootTracker> = RefCell::new(ThreadRootTracker::new());
    }

    pub fn new(root: T) -> Self {
        Self {
            root: ManuallyDrop::new(std::sync::Mutex::new(root)),
            tag: Tag::new(),
        }
    }

    pub fn lock(&self) -> GraphRootGuard<T> {
        let lock = self.root.lock().unwrap();
        GraphRootGuard::new(self.tag, lock)
    }

    /// This root's globally unique tag.
    pub fn tag(&self) -> Tag {
        self.tag
    }
}

impl<T> Drop for Root<T> {
    fn drop(&mut self) {
        Self::THREAD_ROOT_TRACKER.with(|t| {
            {
                let mut t = t.borrow_mut();
                // `root` is effectively locked while we're dropping it, since
                // we hold a mutable reference to it.
                t.add_tag(self.tag);
            }
            // We have to *not* hold the mutable borrow of the tracker while
            // dropping the contents, since the contents Drop implementations
            // may need to validate the tag, which requires they can get
            // (immutable) borrows.
            //
            // SAFETY: Nothing can access root in between this and `self` itself
            // being dropped.
            unsafe { ManuallyDrop::drop(&mut self.root) };
            let mut t = t.borrow_mut();
            t.clear_tag(self.tag);
        })
    }
}

/// Wrapper around a MutexGuard that sets and clears a tag.
pub struct GraphRootGuard<'a, T> {
    tag: Tag,
    guard: MutexGuard<'a, T>,
}

impl<'a, T> GraphRootGuard<'a, T> {
    fn new(tag: Tag, guard: MutexGuard<'a, T>) -> Self {
        Root::<T>::THREAD_ROOT_TRACKER.with(|t| t.borrow_mut().add_tag(tag));
        Self { tag, guard }
    }
}

impl<'a, T> Drop for GraphRootGuard<'a, T> {
    fn drop(&mut self) {
        Root::<T>::THREAD_ROOT_TRACKER.with(|t| t.borrow_mut().clear_tag(self.tag));
    }
}

impl<'a, T> Deref for GraphRootGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}

impl<'a, T> DerefMut for GraphRootGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.deref_mut()
    }
}

/// Analagous to `std::rc::Rc`; in particular like `Rc` and unlike
/// `std::sync::Arc`, it doesn't perform any atomic operations internally (which
/// are moderately expensive).
///
/// Unlike `Rc`, this type `Send` and `Sync` if `T` is. It leverages lock-tracking
/// to ensure `Rc` operations are protected.
///
/// Panics if dropped without the corresponding Root lock being held by the
/// current thread.
pub struct RootedRc<R, T> {
    tag: Tag,
    // TODO: Safety currently relies on assumptions about implementation details
    // of Rc. Probably need to reimplement Rc.
    val: ManuallyDrop<Rc<T>>,
    _phantom: PhantomData<R>,
}

impl<R, T> RootedRc<R, T> {
    /// Creates a new object guarded by the Root with the given `tag`.
    pub fn new(tag: Tag, val: T) -> Self {
        Self {
            tag,
            val: ManuallyDrop::new(Rc::new(val)),
            _phantom: PhantomData,
        }
    }

    /// Uses a reference to the lock to validate safety instead of accessing a
    /// thread-local lock tracker, making it somewhat faster than `clone`.
    ///
    /// Panics if `guard` doesn't match this objects tag.
    pub fn fast_clone(&self, guard: &GraphRootGuard<R>) -> Self {
        assert_eq!(
            guard.tag, self.tag,
            "Tried using a lock for {:?} instead of {:?}",
            guard.tag, self.tag
        );
        // SAFETY: We've verified that the lock is held by inspection of the
        // lock itself. We hold a reference to the guard, guaranteeing that the
        // lock is held while `unchecked_clone` runs.
        unsafe { self.unchecked_clone() }
    }

    // SAFETY: The lock for the root with this object's tag must be held.
    unsafe fn unchecked_clone(&self) -> Self {
        Self {
            tag: self.tag.clone(),
            val: self.val.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<R, T> Clone for RootedRc<R, T> {
    fn clone(&self) -> Self {
        Root::<R>::THREAD_ROOT_TRACKER.with(|t| {
            let t = t.borrow();
            // Validate that the root is locked.
            assert!(
                t.has_tag(self.tag),
                "Clone called without {:?} locked",
                self.tag
            );
            // SAFETY: We've validated that this thread holds the lock.
            // We hold a reference to the tracker, preventing the lock from being
            // released while the clone implementation runs.
            unsafe { self.unchecked_clone() }
        })
    }
}

impl<R, T> Drop for RootedRc<R, T> {
    fn drop(&mut self) {
        Root::<R>::THREAD_ROOT_TRACKER.with(|t| {
            let t = t.borrow();
            // Validate that the root is locked.
            assert!(t.has_tag(self.tag), "Dropped without {:?} locked", self.tag);
            // We have to manually drop `val` while holding the reference
            // to the tracker to ensure the lock can't be released.
            // SAFETY: Nothing can access val in between this and `self` itself
            // being dropped.
            unsafe { ManuallyDrop::drop(&mut self.val) };
        })
    }
}

// SAFETY: Normally the inner `Rc` would inhibit this type from being `Send` and
// `Sync`. However, RootedRc ensures that `Rc`'s reference count can only be
// accessed when the root is locked by the current thread, effectively
// synchronizing the reference count.
unsafe impl<R, T: Send> Send for RootedRc<R, T> {}
unsafe impl<R, T: Sync> Sync for RootedRc<R, T> {}

impl<R, T> Deref for RootedRc<R, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.val.deref()
    }
}

#[cfg(test)]
mod test_rooted_rc {
    use std::thread;

    use super::*;

    #[test]
    fn construct_and_drop() {
        let root = Root::new(());
        let _lock = root.lock();
        let _ = RootedRc::<(), _>::new(root.tag(), 0);
    }

    #[test]
    #[should_panic]
    fn drop_without_lock_panics() {
        let root = Root::new(());
        let _ = RootedRc::<(), _>::new(root.tag(), 0);
    }

    #[test]
    fn send_to_worker_thread() {
        let root = Root::new(());
        let rc = RootedRc::<(), _>::new(root.tag(), 0);
        thread::spawn(move || {
            // Can access immutably without lock.
            let _ = *rc + 2;
            // Need lock to drop, since it mutates refcount.
            let _lock = root.lock();
            drop(rc)
        })
        .join()
        .unwrap();
    }

    #[test]
    fn send_to_worker_thread_and_retrieve() {
        let root = Root::new(());
        let rc = RootedRc::<(), _>::new(root.tag(), 0);
        let root = thread::spawn(move || {
            let _ = *rc;
            let _lock = root.lock();
            drop(rc);
            drop(_lock);
            root
        })
        .join()
        .unwrap();
        // Take the lock to drop rc
        let _lock = root.lock();
    }

    #[test]
    fn clone_to_worker_thread() {
        let root = Root::new(());
        let rc = RootedRc::<(), _>::new(root.tag(), 0);

        // Create a clone of rc that we'll pass to worker thread.
        let rc_thread = {
            let _lock = root.lock();
            rc.clone()
        };

        // Worker takes ownership of rc_thread and root;
        // Returns ownership of root.
        let root = thread::spawn(move || {
            let _ = *rc_thread;
            // Need lock to drop, since it mutates refcount.
            let lock = root.lock();
            drop(rc_thread);
            drop(lock);
            root
        })
        .join()
        .unwrap();

        // Take the lock to drop rc
        {
            let _lock = root.lock();
            drop(rc);
        }
    }
}

pub struct RootedRefCell<R, T> {
    tag: Tag,
    // TODO: Safety currently relies on assumptions about implementation details
    // of RefCell. Probably need to reimplement RefCell.
    val: ManuallyDrop<RefCell<T>>,
    _phantom: PhantomData<R>,
}

impl<R, T> RootedRefCell<R, T> {
    /// Create a RootedRefCell bound to the given tag.
    pub fn new(tag: Tag, val: T) -> Self {
        Self {
            tag,
            val: ManuallyDrop::new(RefCell::new(val)),
            _phantom: PhantomData,
        }
    }

    /// Borrow a reference. Panics if `root_guard` is for the wrong tag, or if
    /// this object is alread mutably borrowed.
    pub fn borrow<'a>(
        &'a self,
        root_guard: &'a GraphRootGuard<'a, R>,
    ) -> RootedRefCellRef<'a, R, T> {
        // Prove that the lock is held for this tag.
        assert_eq!(
            root_guard.tag, self.tag,
            "Expected {:?} Got {:?}",
            self.tag, root_guard.tag
        );
        // Borrow from the guard to ensure the lock can't be dropped.
        RootedRefCellRef {
            _root_guard: root_guard,
            guard: self.val.borrow(),
        }
    }

    /// Borrow a mutable reference. Panics if `root_guard` is for the wrong tag,
    /// or if this object is alread borrowed.
    pub fn borrow_mut<'a>(
        &'a self,
        root_guard: &'a GraphRootGuard<'a, R>,
    ) -> RootedRefCellRefMut<'a, R, T> {
        // Prove that the lock is held for this tag.
        assert_eq!(
            root_guard.tag, self.tag,
            "Expected {:?} Got {:?}",
            self.tag, root_guard.tag
        );
        // Borrow from the guard to ensure the lock can't be dropped.
        RootedRefCellRefMut {
            _root_guard: root_guard,
            guard: self.val.borrow_mut(),
        }
    }
}

unsafe impl<R, T: Send> Send for RootedRefCell<R, T> {}
// Does *not* require  T to be Sync, since we synchronize on the root lock.
unsafe impl<R, T> Sync for RootedRefCell<R, T> {}

impl<R, T> Drop for RootedRefCell<R, T> {
    fn drop(&mut self) {
        Root::<R>::THREAD_ROOT_TRACKER.with(|t| {
            let t = t.borrow();
            // Validate that the root is locked.
            assert!(t.has_tag(self.tag));
            // We have to manually drop `val` while holding the reference
            // to the tracker to ensure the lock can't be released.
            // SAFETY: Nothing can access val in between this and `self` itself
            // being dropped.
            unsafe { ManuallyDrop::drop(&mut self.val) };
        })
    }
}

pub struct RootedRefCellRef<'a, R, T> {
    _root_guard: &'a GraphRootGuard<'a, R>,
    guard: Ref<'a, T>,
}

impl<'a, R, T> Deref for RootedRefCellRef<'a, R, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}

pub struct RootedRefCellRefMut<'a, R, T> {
    _root_guard: &'a GraphRootGuard<'a, R>,
    guard: RefMut<'a, T>,
}

impl<'a, R, T> Deref for RootedRefCellRefMut<'a, R, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}

impl<'a, R, T> DerefMut for RootedRefCellRefMut<'a, R, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.deref_mut()
    }
}

#[cfg(test)]
mod test_rooted_refcell {
    use std::thread;

    use super::*;

    #[test]
    fn construct_and_drop() {
        let root = Root::new(());
        let _lock = root.lock();
        let _ = RootedRefCell::<(), _>::new(root.tag(), 0);
    }

    #[test]
    #[should_panic]
    fn drop_without_lock_panics() {
        let root = Root::new(());
        let _ = RootedRc::<(), _>::new(root.tag(), 0);
    }

    #[test]
    fn share_with_worker_thread() {
        let root = Root::new(());
        let rc = RootedRc::<(), _>::new(root.tag(), RootedRefCell::new(root.tag(), 0));
        let root = {
            let rc = {
                let _lock = root.lock();
                rc.clone()
            };
            thread::spawn(move || {
                let lock = root.lock();
                let mut borrow = rc.borrow_mut(&lock);
                *borrow = 3;
                // Drop rc with lock still held.
                drop(borrow);
                drop(rc);
                drop(lock);
                root
            })
            .join()
            .unwrap()
        };
        // Lock root again ourselves to inspect and drop rc.
        let lock = root.lock();
        let borrow = rc.borrow(&lock);
        assert_eq!(*borrow, 3);
        drop(borrow);
        drop(rc);
        drop(lock);
    }
}
