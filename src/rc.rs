use crate::{GraphRootGuard, Tag};
use std::rc::Rc;

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

impl<T> std::ops::Deref for RootedRc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.val.as_ref().unwrap().deref()
    }
}

#[cfg(test)]
mod test_rooted_rc {
    use std::thread;

    use crate::Root;

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
