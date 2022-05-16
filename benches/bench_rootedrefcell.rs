use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

use atomic_refcell::AtomicRefCell;
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use objgraph::{Root, RootedRc, RootedRefCell};

fn criterion_benchmark(c: &mut Criterion) {
    let root: &'static _ = Box::leak(Box::new(Root::new(())));
    let lock: &'static _ = Box::leak(Box::new(root.lock()));

    {
        let mut group = c.benchmark_group("borrow_mut");
        group.bench_function("RootedRefCell", |b| {
            b.iter_batched(
                || RootedRefCell::<(), _>::new(root.tag(), 0),
                |x| {
                    let rv = *x.borrow_mut(&lock);
                    (x, rv)
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("Mutex", |b| {
            b.iter_batched(
                || Mutex::new(0),
                |x| {
                    let rv = *x.lock().unwrap();
                    (x, rv)
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("parking_lot::Mutex", |b| {
            b.iter_batched(
                || parking_lot::Mutex::new(0),
                |x| {
                    let rv = *x.lock();
                    (x, rv)
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("AtomicRefCell", |b| {
            b.iter_batched(
                || AtomicRefCell::new(0),
                |x| {
                    let rv = *x.borrow_mut();
                    (x, rv)
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("RefCell", |b| {
            b.iter_batched(
                || RefCell::new(0),
                |x| {
                    let rv = *x.borrow_mut();
                    (x, rv)
                },
                BatchSize::SmallInput,
            );
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
