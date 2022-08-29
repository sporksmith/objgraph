/// Prototyping / examples for how this crate may be used in the
/// [shadow](https://github.com/shadow/shadow) simulator.

mod v1 {
    use objgraph::{refcell::RootedRefCell, Root};

    /// Everything related to a single host, stored "flat".
    struct HostObjs {
        root: Root,
        host: RootedRefCell<Host>,
        processes: RootedRefCell<Vec<RootedRefCell<Process>>>,
        threads: RootedRefCell<Vec<RootedRefCell<Thread>>>,
    }

    struct Host {}
    impl Host {
        pub fn run(&mut self, objs: &HostObjs, pid: usize, tid: usize) {
            let processes_guard = objs.processes.borrow(&objs.root);
            let mut process_guard = processes_guard.get(pid).unwrap().borrow_mut(&objs.root);

            // Host bookkeeping

            process_guard.run(objs, self, tid);

            // Host bookkeeping
        }
    }

    struct Process {}
    impl Process {
        pub fn run(&mut self, objs: &HostObjs, host: &mut Host, tid: usize) {
            let threads_guard = objs.threads.borrow(&objs.root);
            let mut thread_guard = threads_guard.get(tid).unwrap().borrow_mut(&objs.root);

            // Process bookkeeping

            thread_guard.run(objs, host, self);

            // Process bookkeeping
        }
    }

    struct Thread {}
    impl Thread {
        pub fn run(&mut self, _objs: &HostObjs, _host: &mut Host, _process: &mut Process) {
            // Do stuff. run, invoke syscall handlers, etc.
        }
    }

    pub fn main() {
        // Create "the world"
        let objs = {
            let root = Root::new();
            let host = RootedRefCell::new(&root, Host {});
            let processes = RootedRefCell::new(
                &root,
                Vec::from([
                    RootedRefCell::new(&root, Process {}),
                    RootedRefCell::new(&root, Process {}),
                ]),
            );
            let threads = RootedRefCell::new(
                &root,
                Vec::from([
                    RootedRefCell::new(&root, Thread {}),
                    RootedRefCell::new(&root, Thread {}),
                ]),
            );
            HostObjs {
                root,
                host,
                processes,
                threads,
            }
        };

        // Run thread tid=0 in process pid=0
        let mut host_guard = objs.host.borrow_mut(&objs.root);
        host_guard.run(&objs, 0, 0);
        // This works ok, but when we have a reference to any single thread or process,
        // we have to immutably borrow the whole list of threads or processes as well.
        //
        // If we needed to mutate those lists, we'd need to
    }
}

/// Similar to above, but wrap individual processes and threads in a RootedRc,
/// allowing us to decouple their lifetimes from the "owning" objects.
///
/// This also allows us to nest the objects within each-other, though we need to
/// be careful to ensure the RootedRc's are dropped explicitly to prevent leaks
/// (or panics in debug builds).
mod v2 {
    use objgraph::{rc::RootedRc, refcell::RootedRefCell, Root};

    /// Everything related to a single host, stored "flat".
    struct HostObjs {
        root: Root,
        host: RootedRefCell<Host>,
    }
    impl Drop for HostObjs {
        fn drop(&mut self) {
            self.host.borrow_mut(&self.root).shutdown(&self.root);
        }
    }

    struct Host {
        processes: RootedRefCell<Vec<RootedRc<RootedRefCell<Process>>>>,
    }
    impl Host {
        pub fn run(&mut self, objs: &HostObjs, pid: usize, tid: usize) {
            let process = self
                .processes
                .borrow(&objs.root)
                .get(pid)
                .unwrap()
                .clone(&objs.root);
            let mut process_guard = process.borrow_mut(&objs.root);

            // Host bookkeeping

            process_guard.run(objs, self, tid);
            drop(process_guard);
            process.safely_drop(&objs.root)

            // Host bookkeeping
        }

        pub fn shutdown(&mut self, root: &Root) {
            let mut processes = self.processes.borrow_mut(root);
            for process in processes.drain(..) {
                process.borrow_mut(root).shutdown(root);
                process.safely_drop(root);
            }
        }
    }

    struct Process {
        threads: RootedRefCell<Vec<RootedRc<RootedRefCell<Thread>>>>,
    }
    impl Process {
        pub fn run(&mut self, objs: &HostObjs, host: &mut Host, tid: usize) {
            let thread = self
                .threads
                .borrow(&objs.root)
                .get(tid)
                .unwrap()
                .clone(&objs.root);
            let mut thread_guard = thread.borrow_mut(&objs.root);

            // Process bookkeeping

            thread_guard.run(objs, host, self);
            drop(thread_guard);
            thread.safely_drop(&objs.root);

            // Process bookkeeping
        }

        pub fn shutdown(&mut self, root: &Root) {
            let mut threads = self.threads.borrow_mut(root);
            for thread in threads.drain(..) {
                thread.safely_drop(root)
            }
        }
    }

    struct Thread {}
    impl Thread {
        pub fn run(&mut self, _objs: &HostObjs, _host: &mut Host, _process: &mut Process) {
            // Do stuff. run, invoke syscall handlers, etc.
        }
    }

    pub fn main() {
        // Create "the world"
        let objs = {
            let root = Root::new();
            let threads = RootedRefCell::new(
                &root,
                Vec::from([
                    RootedRc::new(&root, RootedRefCell::new(&root, Thread {})),
                    RootedRc::new(&root, RootedRefCell::new(&root, Thread {})),
                ]),
            );
            let processes = RootedRefCell::new(
                &root,
                Vec::from([RootedRc::new(
                    &root,
                    RootedRefCell::new(&root, Process { threads }),
                )]),
            );
            let host = RootedRefCell::new(&root, Host { processes });
            HostObjs { root, host }
        };

        // Run thread tid=0 in process pid=0
        let mut host_guard = objs.host.borrow_mut(&objs.root);
        host_guard.run(&objs, 0, 0);
    }
}

pub fn main() {
    v1::main();
    v2::main();
}

// For `cargo test --examples`
#[test]
fn test() {
    main();
}
