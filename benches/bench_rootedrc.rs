use std::{sync::Arc, rc::Rc};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use objgraph::{Root, RootedRc};

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("RootedRc::clone 1000", |b| b.iter(|| {
        let root = Root::new(());
        let _lock = root.lock();
        let x = RootedRc::new(root.tag(), 0);
        for _ in 0..black_box(1000) {
            let _ = x.clone();
        }
    }));
    c.bench_function("Arc::clone 1000", |b| b.iter(|| {
        let x = Arc::new(0);
        for _ in 0..black_box(1000) {
            let _ = x.clone();
        }
    }));
    c.bench_function("rc::clone 1000", |b| b.iter(|| {
        let x = Rc::new(0);
        for _ in 0..black_box(1000) {
            let _ = x.clone();
        }
    }));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);