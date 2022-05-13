use std::cell::RefCell;
use std::{
    collections::HashSet,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard,
    },
};

/// Every object root is assigned a Tag, which we enforce is globally unique.
type Tag = u64;
type AtomicTag = AtomicU64;

/// For simplicity we have a single SUPER_ROOT, which enforces uniqueness of
/// `Tag`s.
///
/// It'd probably be a little nicer to let crate-users provide their own
/// `SuperRoot`, but then we need a bit more tracking and checking to ensure
/// we're not comparing tags from different `SuperRoot`s.
///
/// Conversely, since tags are 64 bits, multiple users of this crate shouldn't
/// interfere with eachother. At worst maybe there could be contention for the
/// underlying `Atomic`?
struct SuperRoot {
    next_tag: AtomicTag,
}
static SUPER_ROOT: SuperRoot = SuperRoot::new();

impl SuperRoot {
    pub const fn new() -> Self {
        Self {
            next_tag: AtomicTag::new(0),
        }
    }

    fn next_tag(&self) -> Tag {
        self.next_tag.fetch_add(1, Ordering::Relaxed)
    }
}

/// Tracks which `Tag`s (and hence which `Root`s) are locked by the current thread.
///
/// Again it might be nicer to let crate users provide their own. In that case
/// we could probably also simplify the implementation to just store an
/// `Option<Tag>`, avoiding the overhead of the `HashSet` in cases where it's
/// unneeded.
///
/// To do that we'd again need some additional tracking to ensure we're checking
/// the right object. This is made a bit more complex because typically this
/// should be kept in a thread-local, which we can't really store references for
/// elsewhere.
struct ThreadRootTracker {
    current_tags: HashSet<Tag>,
    // Force to *not* be Send nor Sync, since it tracks state of the *current
    // thread*.
    // Probably not strictly necessary while this struct is private, since we
    // already only ever store instances in a thread-local.
    _phantom: std::marker::PhantomData<*mut std::ffi::c_void>,
}

impl ThreadRootTracker {
    pub fn new() -> Self {
        Self {
            current_tags: HashSet::new(),
            _phantom: std::marker::PhantomData,
        }
    }

    fn has_tag(&self, tag: Tag) -> bool {
        self.current_tags.contains(&tag)
    }

    fn add_tag(&mut self, tag: Tag) {
        self.current_tags.insert(tag);
    }

    fn clear_tag(&mut self, tag: Tag) {
        self.current_tags.remove(&tag);
    }
}

thread_local! {
    /// Must be unique per thread. Must also be accessible by e.g.
    /// `RootedRc::Drop`, which is easiest if it's in a thread-local.
    ///
    /// TODO: Add a way for crate-users to supply their own tracker.
    static THREAD_ROOT_TRACKER : RefCell<ThreadRootTracker> = RefCell::new(ThreadRootTracker::new());
}

/// Root of an "object graph". It holds a lock over the contents of the graph,
/// and ensures tracks which tags are locked by the current thread.
pub struct Root<T> {
    root: ManuallyDrop<Mutex<T>>,
    tag: Tag,
}

impl<T> Root<T> {
    pub fn new(root: T) -> Self {
        Self {
            root: ManuallyDrop::new(std::sync::Mutex::new(root)),
            tag: SUPER_ROOT.next_tag(),
        }
    }

    pub fn lock(&self) -> GraphRootGuard<T> {
        let lock = self.root.lock().unwrap();
        GraphRootGuard::new(self.tag, lock)
    }

    // TODO: Maybe avoid exposing the tag publicly, and/or make its type opaque?
    // I don't think the current state allows users to break soundness; it's
    // just leaking implementation details here a bit.
    pub fn tag(&self) -> Tag {
        self.tag
    }
}

impl<T> Drop for Root<T> {
    fn drop(&mut self) {
        THREAD_ROOT_TRACKER.with(|t| {
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
        THREAD_ROOT_TRACKER.with(|t| t.borrow_mut().add_tag(tag));
        Self { tag, guard }
    }
}

impl<'a, T> Drop for GraphRootGuard<'a, T> {
    fn drop(&mut self) {
        THREAD_ROOT_TRACKER.with(|t| t.borrow_mut().clear_tag(self.tag));
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
pub struct RootedRc<T> {
    tag: Tag,
    val: ManuallyDrop<Rc<T>>,
}

impl<T> RootedRc<T> {
    pub fn new(tag: Tag, val: T) -> Self {
        Self {
            tag,
            val: ManuallyDrop::new(Rc::new(val)),
        }
    }
}

impl<T> Clone for RootedRc<T> {
    fn clone(&self) -> Self {
        THREAD_ROOT_TRACKER.with(|t| {
            let t = t.borrow();
            // Validate that the root is locked.
            assert!(t.has_tag(self.tag));
            // Continue holding a reference to the tracker while calling member
            // methods, to ensure the lock isn't dropped while they're running.
            Self {
                tag: self.tag.clone(),
                val: self.val.clone(),
            }
        })
    }
}

impl<T> Drop for RootedRc<T> {
    fn drop(&mut self) {
        THREAD_ROOT_TRACKER.with(|t| {
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

// SAFETY: Normally the inner `Rc` would inhibit this type from being `Send` and
// `Sync`. However, RootedRc ensures that `Rc`'s reference count can only be
// accessed when the root is locked by the current thread, effectively
// synchronizing the reference count.
unsafe impl<T: Send> Send for RootedRc<T> {}
unsafe impl<T: Sync> Sync for RootedRc<T> {}

impl<T> Deref for RootedRc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.val.deref()
    }
}

// TODO: RootedRefCell.
