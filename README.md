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

## Usage and testing

There is a mock-up example of Shadow's intended usage of this crate in
`examples/shadow.rs`, which can be run with `cargo run --example shadow`. It
also passes [miri](https://github.com/rust-lang/miri) (`cargo miri run --example shadow`).

There are also unit tests, which should also pass `miri`, with
`-Zmiri-ignore-leaks`. See https://github.com/sporksmith/objgraph/issues/1

## Status

This is currently a sketch for discussion and analysis. It needs more review and testing
to validate soundness, and so far the performance benefit vs just using `Arc` 
appear to be marginal. e.g. see https://github.com/sporksmith/objgraph/issues/2#issuecomment-1126967984. (It may be worth also building `RootedRefCell` to compare
it to `RefCell` and `Mutex`, but I'd expect the performance tradeoff to be similar)

There is also a lot of room for ergonomic and performance improvements for this
to work well as a general-purpose crate.
