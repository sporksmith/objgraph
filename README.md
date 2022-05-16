This is a proof of concept for safe, efficient object graphs in Rust.  It is
inspired by the concurrency model used in
[Shadow's](https://github.com/shadow/shadow) C implementation, and is intended
as a potential path toward migrating Shadow's C code to Rust without first
having to extensively refactor and/or introduce a lot of atomic operations.

Shadow simulates a network of Hosts, each of which has a lock associated with
it.  Inside the Hosts are a graph of ref-counted objects. They are meant to only
be accessed with the corresponding Host lock held, and do *not* take additional
locks when manipulating the reference counts.

Hosts are sent across Worker threads over the course of a simulation.

Translating this model to Rust, we can't simply use `Rc` for the reference counts,
since the Hosts would then not be `Send`.

We could use `Arc`, but this would introduce a lot of new costly atomic operations.

Here we encode Shadow's original safety model into Rust's type system. Each host
in Shadow becomes a `crate::Root`. Reference counting is done with
`crate::RootedRc`, which is functionally an `Rc`, but has runtime checks to
ensure that the reference count is only ever manipulated with the owning
`Root`'s lock held. We mark `crate::RootedRc` as `Send` and `Sync`, allowing it
to be sent across threads.

We should be able to similarly implement `RootedRefCell` to allow us to do `RefCell`-like
borrow tracking without atomic operations, while retaining `Send` and `Sync`.

## Performance And Send/Sync

`RootedRc::clone` is only marginally faster than `Arc::clone`;
`RootedRc::fast_clone` is faster but requires a reference to the `Root`
object's lock.

From fastest to slowest:

| benchmark | time | Send | Sync |
| -------- | ------ | -- | -- |
| clone/Rc                  | 1.2097 ns 1.2526 ns 1.2923 ns | !Send | !Sync |
| **clone/RootedRc fast_clone** | 5.5402 ns 5.5796 ns 5.6151 ns | Send where T: Sync + Send | Sync where T: Sync + Send |
| **clone/RootedRc**             | 8.6915 ns 8.7197 ns 8.7470 ns | Send where T: Sync + Send | Sync where T: Sync + Send |
| clone/Arc                  | 10.613 ns 10.648 ns 10.689 ns | Send where T: Sync + Send |  Sync where T: Sync + Send |


Performance for `RootedRefCell` is analagous to `RootedRc::fast_clone`,
since it always requires a reference to the lock to borrow (thus ensuring
that the lock is held for the entire time that the borrowed reference is).

From fastest to slowest:

| benchmark | time | Send | Sync |
| -------- | ------ | -- | -- |
| borrow_mut/RefCell       | 1.6174 ns 1.6543 ns 1.6840 ns | Send where T: Send | !Sync |
| **borrow_mut/RootedRefCell** | 5.9403 ns 5.9613 ns 5.9800 ns | Send where T: Send | Sync where T: Send |
| borrow_mut/AtomicRefCell | 10.912 ns 10.928 ns 10.942 ns | Send where T: Send | Sync where T: Send |
| borrow_mut/parking_lot::Mutex | 13.187 ns 13.209 ns 13.229 ns | Send where T: Send | Sync where T: Send |
| borrow_mut/Mutex         | 19.187 ns 19.203 ns 19.219 ns | Send where T: Send | Sync where T: Send |

Benchmark sources are in `benches` and can be run with `cargo bench`.

## Usage and testing

There is a mock-up example of Shadow's intended usage of this crate in
`examples/shadow.rs`, which can be run with `cargo run --example shadow`. It
also passes [miri](https://github.com/rust-lang/miri) (`cargo miri run --example shadow`).

There are also unit tests, which should also pass `miri`, with
`-Zmiri-ignore-leaks`. See https://github.com/sporksmith/objgraph/issues/1

## Status

This is currently a sketch for discussion and analysis. It needs more review
and testing to validate soundness.

There is also a lot of room for ergonomic and performance improvements for this
to work well as a general-purpose crate.
