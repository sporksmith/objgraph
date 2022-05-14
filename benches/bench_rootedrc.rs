#![feature(thread_local)]

use std::{rc::Rc, sync::Arc};

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use objgraph::{Root, RootedRc};

fn criterion_benchmark(c: &mut Criterion) {
    let root = Root::new(());
    let _lock = root.lock();

    {
        let mut group = c.benchmark_group("clone");
        group.bench_function("RootedRc", |b| {
            b.iter_batched(
                || RootedRc::new(root.tag(), ()),
                |x| x.clone(),
                BatchSize::SmallInput,
            );
        });
        group.bench_function("Arc", |b| {
            b.iter_batched(|| Arc::new(()), |x| x.clone(), BatchSize::SmallInput);
        });
        group.bench_function("Rc", |b| {
            b.iter_batched(|| Rc::new(()), |x| x.clone(), BatchSize::SmallInput);
        });
    }

    {
        let mut group = c.benchmark_group("drop");
        group.bench_function("RootedRc", |b| {
            b.iter_batched(
                || RootedRc::new(root.tag(), ()),
                |x| drop(x),
                BatchSize::SmallInput,
            );
        });
        group.bench_function("Arc", |b| {
            b.iter_batched(|| Arc::new(()), |x| drop(x), BatchSize::SmallInput);
        });
        group.bench_function("Rc", |b| {
            b.iter_batched(|| Rc::new(()), |x| drop(x), BatchSize::SmallInput);
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
