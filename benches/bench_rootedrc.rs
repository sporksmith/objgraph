use std::{rc::Rc, sync::Arc};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use objgraph::{Root, RootedRc};

fn do_clone<T: Clone>(iterations: usize, val: T) {
    for _ in 0..black_box(iterations) {
        let _ = val.clone();
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("RootedRc::clone 1000", |b| {
        b.iter(|| {
            let root = Root::new(());
            let _lock = root.lock();
            let x = RootedRc::new(root.tag(), 0);
            do_clone(black_box(1000), x);
        })
    });
    c.bench_function("Arc::clone 1000", |b| {
        b.iter(|| {
            let x = Arc::new(());
            do_clone(black_box(1000), x);
        })
    });
    c.bench_function("Rc::clone 1000", |b| {
        b.iter(|| {
            let x = Rc::new(());
            do_clone(black_box(1000), x);
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
