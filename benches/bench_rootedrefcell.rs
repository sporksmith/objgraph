use std::{cell::RefCell, sync::Mutex};

use atomic_refcell::AtomicRefCell;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use objgraph::{Root, RootedRefCell};

fn criterion_benchmark(c: &mut Criterion) {
    let root: &'static _ = Box::leak(Box::new(Root::new()));
    let lock: &'static _ = Box::leak(Box::new(root.lock()));

    {
        let mut group = c.benchmark_group("borrow_mut");
        group.bench_function("RootedRefCell", |b| {
            b.iter_batched_ref(
                || RootedRefCell::new(root.tag(), 0),
                |x| {
                    *x.borrow_mut(&lock) += 1;
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("Mutex", |b| {
            b.iter_batched_ref(
                || Mutex::new(0),
                |x| {
                    *x.lock().unwrap() += 1;
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("parking_lot::Mutex", |b| {
            b.iter_batched_ref(
                || parking_lot::Mutex::new(0),
                |x| {
                    *x.lock() += 1;
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("AtomicRefCell", |b| {
            b.iter_batched_ref(
                || AtomicRefCell::new(0),
                |x| {
                    *x.borrow_mut() += 1;
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("RefCell", |b| {
            b.iter_batched_ref(
                || RefCell::new(0),
                |x| {
                    *x.borrow_mut() += 1;
                },
                BatchSize::SmallInput,
            );
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
