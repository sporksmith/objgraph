use std::{cell::RefCell, sync::Mutex};

use atomic_refcell::AtomicRefCell;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use objgraph::{refcell::RootedRefCell, Root};

#[inline(never)]
fn rootedrefcell_borrow_mut(root: &Root, x: &RootedRefCell<i32>) {
    *x.borrow_mut(root) += 1;
}

#[inline(never)]
fn mutex_borrow_mut(x: &Mutex<i32>) {
    *x.lock().unwrap() += 1;
}

#[inline(never)]
fn parking_lot_mutex_borrow_mut(x: &parking_lot::Mutex<i32>) {
    *x.lock() += 1;
}

#[inline(never)]
fn atomicrefcell_borrow_mut(x: &AtomicRefCell<i32>) {
    *x.borrow_mut() += 1;
}

#[inline(never)]
fn refcell_borrow_mut(x: &RefCell<i32>) {
    *x.borrow_mut() += 1;
}

fn criterion_benchmark(c: &mut Criterion) {
    {
        let mut group = c.benchmark_group("borrow_mut");
        group.bench_function("RootedRefCell", |b| {
            b.iter_batched_ref(
                || {
                    let root = Root::new();
                    let x = RootedRefCell::new(&root, 0);
                    (root, x)
                },
                |(root, x)| rootedrefcell_borrow_mut(root, x),
                BatchSize::SmallInput,
            );
        });
        group.bench_function("Mutex", |b| {
            b.iter_batched_ref(
                || Mutex::new(0),
                |x| mutex_borrow_mut(x),
                BatchSize::SmallInput,
            );
        });
        group.bench_function("parking_lot::Mutex", |b| {
            b.iter_batched_ref(
                || parking_lot::Mutex::new(0),
                |x| parking_lot_mutex_borrow_mut(x),
                BatchSize::SmallInput,
            );
        });
        group.bench_function("AtomicRefCell", |b| {
            b.iter_batched_ref(
                || AtomicRefCell::new(0),
                |x| atomicrefcell_borrow_mut(x),
                BatchSize::SmallInput,
            );
        });
        group.bench_function("RefCell", |b| {
            b.iter_batched_ref(
                || RefCell::new(0),
                |x| refcell_borrow_mut(x),
                BatchSize::SmallInput,
            );
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
