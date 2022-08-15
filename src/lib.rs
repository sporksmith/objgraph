// https://github.com/rust-lang/rfcs/blob/master/text/2585-unsafe-block-in-unsafe-fn.md
#![deny(unsafe_op_in_unsafe_fn)]

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Mutex, MutexGuard,
};

use once_cell::sync::OnceCell;

/// Every object root is assigned a Tag, which we ensure is globally unique.
/// Each Tag value uniquely identifies a Root.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Tag {
    prefix: TagPrefixType,
    suffix: TagSuffixType,
}

/// Larger sizes here reduce the chance of collision, which could lead to
/// silently missing bugs in some cases. Note though that there would both
/// have to be a collision, and the code would need to incorrectly try to
/// access data using the wrong root lock.
///
/// Increasing the size introduces some runtime overhead for storing, copying,
/// and comparing tag values.
type TagPrefixType = u32;

/// Larger sizes here support a greater number of tags within a given prefix.
///
/// Increasing the size introduces some runtime overhead for storing, copying,
/// and comparing tag values.
type TagSuffixType = u32;
type TagSuffixAtomicType = AtomicU32;

impl Tag {
    pub fn new() -> Self {
        // Every instance of this module uses a random prefix for tags.  This is to
        // handle both the case where this module is used from multiple processes that
        // share memory, and to handle the case where multiple instances of this module
        // end up within a single process.
        static TAG_PREFIX: OnceCell<TagPrefixType> = OnceCell::new();
        let prefix = *TAG_PREFIX.get_or_init(|| rand::prelude::random());

        static NEXT_TAG_SUFFIX: TagSuffixAtomicType = TagSuffixAtomicType::new(0);
        let suffix: TagSuffixType = NEXT_TAG_SUFFIX.fetch_add(1, Ordering::Relaxed);

        // Detect overflow
        assert!(suffix != TagSuffixType::MAX);

        Self { prefix, suffix }
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
