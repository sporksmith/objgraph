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

`RootedRc` is roughly as fast as `Rc`, and about half the cost of `Arc`. (It's currently measuring
consistently faster than `Rc`, which seems like a measurement artifact, but I haven't been able to identify it.) From fastest to slowest:

| benchmark | time | Send | Sync |
| -------- | ------ | -- | -- |
| **clone and drop/RootedRc** | 12.599 ns 12.669 ns 12.739  | Send where T: Sync + Send | Sync where T: Sync + Send |
| clone and drop/Rc                  | 14.552 ns 14.732 ns 14.911 ns | !Send | !Sync |
| clone and drop/Arc                  | 29.717 ns 29.853 ns 30.000 ns | Send where T: Sync + Send |  Sync where T: Sync + Send |


`RootedRefCell` is a bit slower than `RefCell`, but again about half the cost of the fastest synchronized
equivalent, `AtomicRefCell`.

From fastest to slowest:

| benchmark | time | Send | Sync |
| -------- | ------ | -- | -- |
| borrow_mut/RefCell       | 988.27 ps 1.0092 ns 1.0258 ns | Send where T: Send | !Sync |
| **borrow_mut/RootedRefCell** | 1.5664 ns 1.6050 ns 1.6371 ns | Send where T: Send | Sync where T: Send |
| borrow_mut/AtomicRefCell | 6.0423 ns 6.0533 ns 6.0643 ns | Send where T: Send | Sync where T: Send |
| borrow_mut/parking_lot::Mutex | 9.9365 ns 9.9496 ns 9.9647 ns | Send where T: Send | Sync where T: Send |
| borrow_mut/Mutex         |  10.462 ns 10.476 ns 10.492 ns| Send where T: Send | Sync where T: Send |

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
