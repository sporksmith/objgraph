// https://github.com/rust-lang/rfcs/blob/master/text/2585-unsafe-block-in-unsafe-fn.md
#![deny(unsafe_op_in_unsafe_fn)]

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, MutexGuard,
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
            root: std::sync::Mutex::new(InnerRoot { tag }),
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
pub mod rc;
pub mod refcell;
