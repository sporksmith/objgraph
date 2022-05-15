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
                        let core_ids = core_affinity::get_core_ids().unwrap();
                        let setup_core_id = core_ids[0];
                        // Run test on a separate cpu. core_ids[1] might
                        // be on the same core as 0 (hyperthreading), so skip to 2.
                        let test_core_id = core_ids[2];
                        core_affinity::set_for_current(setup_core_id);
                        let mut v = Vec::new();
                        let _lock = root.lock();
                        for _ in 0..black_box(N) {
                            v.push(RootedRc::new(root.tag(), ()));
                            // Force an atomic operation on this core.
                            let _ = v.last().unwrap().clone();
                        }
                        drop(_lock);
                        (root, test_core_id, v)
                    }).join().unwrap()
                },
                |(root, test_core_id, v)| {
                    std::thread::spawn(move || {
                        core_affinity::set_for_current(test_core_id);
                        let _lock = root.lock();
                        for rc in v {
                            let _ = rc.clone();
                        }
                    }).join().unwrap()
                },
                BatchSize::SmallInput,
            );
        });
        group.bench_function("Arc", |b| {
            b.iter_batched(
                || {
                    std::thread::spawn(|| {
                        let core_ids = core_affinity::get_core_ids().unwrap();
                        let setup_core_id = core_ids[0];
                        // Run test on a separate cpu. core_ids[1] might
                        // be on the same core as 0 (hyperthreading), so skip to 2.
                        let test_core_id = core_ids[2];
                        core_affinity::set_for_current(setup_core_id);
                        let mut v = Vec::new();
                        for _ in 0..black_box(N) {
                            v.push(Arc::new(()));
                            // Force an atomic operation on this core.
                            let _ = v.last().unwrap().clone();
                        }
                        (test_core_id, v)
                    }).join().unwrap()
                },
                |(test_core_id, v)| {
                    std::thread::spawn(move || {
                        core_affinity::set_for_current(test_core_id);
                        for rc in v {
                            let _ = rc.clone();
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
