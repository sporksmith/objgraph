#![feature(thread_local)]

use std::{rc::Rc, sync::Arc};

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, black_box};
use objgraph::{Root, RootedRc};

fn criterion_benchmark(c: &mut Criterion) {
    let root = Root::new(());

    {
        let _lock = root.lock();
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
        let mut group = c.benchmark_group("cross-core clone clone");
        const N : usize = 10000;
        group.bench_function("RootedRc", |b| {
            b.iter_batched(
                || {
                    std::thread::spawn(|| {
                        let root = Root::new(());
                        let mut core_ids = core_affinity::get_core_ids().unwrap();
                        // Exclude Current core from tests.
                        let setup_core_id = core_ids.pop().unwrap();
                        core_affinity::set_for_current(setup_core_id);
                        let mut v = Vec::new();
                        let _lock = root.lock();
                        for _ in 0..black_box(N) {
                            v.push(RootedRc::new(root.tag(), ()));
                            // Force an atomic operation on this core.
                            let _ = v.last().unwrap().clone();
                        }
                        drop(_lock);
                        (root, core_ids, v)
                    }).join().unwrap()
                },
                |(root, core_ids, v)| {
                    std::thread::spawn(move || {
                        let _lock = root.lock();
                        for core_id in core_ids {
                            core_affinity::set_for_current(core_id);
                            for rc in &v {
                                let _ = rc.clone();
                            }
                        }
                        // Drop v with lock still held.
                        drop(v);
                    }).join().unwrap()
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("Arc", |b| {
            b.iter_batched(
                || {
                    std::thread::spawn(|| {
                        let mut core_ids = core_affinity::get_core_ids().unwrap();
                        // Exclude Current core from tests.
                        let setup_core_id = core_ids.pop().unwrap();
                        core_affinity::set_for_current(setup_core_id);
                        let mut v = Vec::new();
                        for _ in 0..black_box(N) {
                            v.push(Arc::new(()));
                            // Force an atomic operation on this core.
                            let _ = v.last().unwrap().clone();
                        }
                        (core_ids, v)
                    }).join().unwrap()
                },
                |(core_ids, v)| {
                    std::thread::spawn(move || {
                        for core_id in core_ids {
                            core_affinity::set_for_current(core_id);
                            for rc in &v {
                                let _ = rc.clone();
                            }
                        }
                    }).join().unwrap()
                },
                BatchSize::SmallInput,
            );
        });
    }

    {
        let _lock = root.lock();
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
