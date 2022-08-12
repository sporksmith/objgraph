use std::cell::{Ref, RefCell, RefMut};
use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard,
    },
};

/// Every object root is assigned a Tag, which we enforce is globally unique.
/// XXX: todo: incorporate pid?
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Tag(u64);
static NEXT_TAG: AtomicU64 = AtomicU64::new(0);
impl Tag {
    pub fn new() -> Self {
        Self(NEXT_TAG.fetch_add(1, Ordering::Relaxed))
    }
}

struct InnerRoot {
    tag: Tag,
}

/// Root of an "object graph". It holds a lock over the contents of the graph,
/// and ensures tracks which tags are locked by the current thread.
///
/// We only support a thread having one Root of any given type T being locked at
/// once. Crate users should use a private type T that they own to avoid
/// conflicts.
pub struct Root {
    root: Mutex<InnerRoot>,
    tag: Tag,
}

impl Root {
    pub fn new() -> Self {
        let tag = Tag::new();
        Self {
            root: std::sync::Mutex::new(InnerRoot { tag } ),
            tag,
        }
    }

    pub fn lock(&self) -> GraphRootGuard {
        let lock = self.root.lock().unwrap();
        GraphRootGuard::new(lock)
    }

    /// This root's globally unique tag.
    pub fn tag(&self) -> Tag {
        self.tag
    }
}

/// Wrapper around a MutexGuard that sets and clears a tag.
pub struct GraphRootGuard<'a> {
    guard: MutexGuard<'a, InnerRoot>,
}

impl<'a> GraphRootGuard<'a> {
    fn new(guard: MutexGuard<'a, InnerRoot>) -> Self {
        Self { guard }
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
pub struct RootedRc<T> {
    tag: Tag,
    // TODO: Safety currently relies on assumptions about implementation details
    // of Rc. Probably need to reimplement Rc.
    val: Option<Rc<T>>,
}

impl<T> RootedRc<T> {
    /// Creates a new object guarded by the Root with the given `tag`.
    pub fn new(tag: Tag, val: T) -> Self {
        Self {
            tag,
            val: Some(Rc::new(val)),
        }
    }

    /// Like Clone::clone, but requires that the corresponding Root is locked.
    /// 
    /// Intentionally named clone to shadow Self::deref()::clone().
    ///
    /// Panics if `guard` doesn't match this objects tag.
    pub fn clone(&self, guard: &GraphRootGuard) -> Self {
        assert_eq!(
            guard.guard.tag, self.tag,
            "Tried using a lock for {:?} instead of {:?}",
            guard.guard.tag, self.tag
        );
        // SAFETY: We've verified that the lock is held by inspection of the
        // lock itself. We hold a reference to the guard, guaranteeing that the
        // lock is held while `unchecked_clone` runs.
        unsafe { self.unchecked_clone() }
    }

    // SAFETY: The lock for the root with this object's tag must be held.
    pub unsafe fn unchecked_clone(&self) -> Self {
        Self {
            tag: self.tag.clone(),
            val: self.val.clone(),
        }
    }

    pub fn safely_drop(mut self, guard: &GraphRootGuard) {
        assert_eq!(
            guard.guard.tag, self.tag,
            "Tried using a lock for {:?} instead of {:?}",
            guard.guard.tag, self.tag
        );
        self.val.take();
    }
}

impl<T> Drop for RootedRc<T> {
    fn drop(&mut self) {
        if let Some(val) = self.val.take() {
            // Unsafe to access val's contents. Leak them.
            std::mem::forget(val);
            // XXX: Maybe just log in release builds?
            panic!("Dropped without calling `safely_drop`");
        }
    }
}

// SAFETY: Normally the inner `Rc` would inhibit this type from being `Send` and
// `Sync`. However, RootedRc ensures that `Rc`'s reference count can only be
// accessed when the root is locked by the current thread, effectively
// synchronizing the reference count.
unsafe impl<T: Sync + Send> Send for RootedRc<T> {}
unsafe impl<T: Sync + Send> Sync for RootedRc<T> {}

impl<T> Deref for RootedRc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.val.as_ref().unwrap().deref()
    }
}

#[cfg(test)]
mod test_rooted_rc {
    use std::thread;

    use super::*;

    #[test]
    fn construct_and_drop() {
        let root = Root::new();
        let lock = root.lock();
        let rc = RootedRc::new(root.tag(), 0);
        rc.safely_drop(&lock)
    }

    #[test]
    #[should_panic]
    fn drop_without_lock_panics() {
        let root = Root::new();
        let _ = RootedRc::new(root.tag(), 0);
    }

    #[test]
    fn send_to_worker_thread() {
        let root = Root::new();
        let rc = RootedRc::new(root.tag(), 0);
        thread::spawn(move || {
            // Can access immutably without lock.
            let _ = *rc + 2;
            // Need lock to drop, since it mutates refcount.
            let lock = root.lock();
            rc.safely_drop(&lock);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn send_to_worker_thread_and_retrieve() {
        let root = Root::new();
        let root = thread::spawn(move || {
            let rc = RootedRc::new(root.tag(), 0);
            rc.safely_drop(&root.lock());
            root
        })
        .join()
        .unwrap();
        let rc = RootedRc::new(root.tag(), 0);
        rc.safely_drop(&root.lock());
    }

    #[test]
    fn clone_to_worker_thread() {
        let root = Root::new();
        let rc = RootedRc::new(root.tag(), 0);

        // Create a clone of rc that we'll pass to worker thread.
        let rc_thread = rc.clone(&root.lock());

        // Worker takes ownership of rc_thread and root;
        // Returns ownership of root.
        let root = thread::spawn(move || {
            let _ = *rc_thread;
            // Need lock to drop, since it mutates refcount.
            rc_thread.safely_drop(&root.lock());
            root
        })
        .join()
        .unwrap();

        // Take the lock to drop rc
        rc.safely_drop(&root.lock());
    }
}

pub struct RootedRefCell<T> {
    tag: Tag,
    // TODO: Safety currently relies on assumptions about implementation details
    // of RefCell. Probably need to reimplement RefCell.
    val: RefCell<T>,
}

impl<T> RootedRefCell<T> {
    /// Create a RootedRefCell bound to the given tag.
    pub fn new(tag: Tag, val: T) -> Self {
        Self {
            tag,
            val: RefCell::new(val),
        }
    }

    /// Borrow a reference. Panics if `root_guard` is for the wrong tag, or if
    /// this object is alread mutably borrowed.
    pub fn borrow<'a>(
        &'a self,
        // This 'a statically enforces that the root lock can't be dropped
        // while the returned guard is still outstanding. i.e. it is part
        // of the safety proof for making Self Send and Sync.
        //
        // Alternatively we could drop that requirement and add a dynamic check.
        root_guard: &'a GraphRootGuard<'a>,
    ) -> RootedRefCellRef<'a, T> {
        // Prove that the lock is held for this tag.
        assert_eq!(
            root_guard.guard.tag, self.tag,
            "Expected {:?} Got {:?}",
            self.tag, root_guard.guard.tag
        );
        // Borrow from the guard to ensure the lock can't be dropped.
        RootedRefCellRef {
            guard: self.val.borrow(),
        }
    }

    /// Borrow a mutable reference. Panics if `root_guard` is for the wrong tag,
    /// or if this object is already borrowed.
    pub fn borrow_mut<'a>(
        &'a self,
        // 'a required here for safety, as for `borrow`.
        root_guard: &'a GraphRootGuard<'a>,
    ) -> RootedRefCellRefMut<'a, T> {
        // Prove that the lock is held for this tag.
        assert_eq!(
            root_guard.guard.tag, self.tag,
            "Expected {:?} Got {:?}",
            self.tag, root_guard.guard.tag
        );
        RootedRefCellRefMut {
            guard: self.val.borrow_mut(),
        }
    }
}

unsafe impl<T: Send> Send for RootedRefCell<T> {}
unsafe impl<T: Send> Sync for RootedRefCell<T> {}

pub struct RootedRefCellRef<'a, T> {
    guard: Ref<'a, T>,
}

impl<'a, T> Deref for RootedRefCellRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}

pub struct RootedRefCellRefMut<'a, T> {
    guard: RefMut<'a, T>,
}

impl<'a, T> Deref for RootedRefCellRefMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.deref()
    }
}

impl<'a, T> DerefMut for RootedRefCellRefMut<'a, T> {
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
        let root = Root::new();
        let _lock = root.lock();
        let _ = RootedRefCell::new(root.tag(), 0);
    }

    #[test]
    fn share_with_worker_thread() {
        let root = Root::new();
        let rc = RootedRc::new(root.tag(), RootedRefCell::new(root.tag(), 0));
        let root = {
            let rc = {
                let lock = root.lock();
                rc.clone(&lock)
            };
            thread::spawn(move || {
                let lock = root.lock();
                let mut borrow = rc.borrow_mut(&lock);
                *borrow = 3;
                // Drop rc with lock still held.
                drop(borrow);
                rc.safely_drop(&lock);
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
        rc.safely_drop(&lock);
        drop(lock);
    }
}