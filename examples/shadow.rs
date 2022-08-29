/// Sketch of how shared ownership of Descriptors might look in
/// the [shadow](https://github.com/shadow/shadow) simulator.
use objgraph::{rc::RootedRc, Root};
use std::{collections::HashMap, thread};

struct Host {
    processes: HashMap<u32, Process>,
    root: Root,
}

impl Drop for Host {
    fn drop(&mut self) {
        for (_, p) in self.processes.drain() {
            p.safely_drop(&self.root);
        }
    }
}

struct Process {
    descriptors: HashMap<u32, RootedRc<Descriptor>>,
}

impl Process {
    pub fn safely_drop(self, root: &Root) {
        for (_, d) in self.descriptors {
            d.safely_drop(root)
        }
    }
}

struct Descriptor {
    open: bool,
}

pub fn main() {
    let mut hosts = HashMap::<u32, Host>::new();

    // host1 has 2 processes, which have a shared Descriptor.
    // (Maybe one was forked from the other)
    let mut host1 = Host {
        processes: HashMap::new(),
        root: Root::new(),
    };
    {
        let descriptor = RootedRc::new(&host1.root, Descriptor { open: true });

        // Process 0 has a reference to the descriptor.
        host1.processes.insert(
            0,
            Process {
                descriptors: HashMap::new(),
            },
        );
        host1
            .processes
            .get_mut(&0)
            .unwrap()
            .descriptors
            .insert(0, descriptor.clone(&host1.root));

        // So does Process 1.
        host1.processes.insert(
            1,
            Process {
                descriptors: HashMap::new(),
            },
        );
        host1
            .processes
            .get_mut(&1)
            .unwrap()
            .descriptors
            .insert(0, descriptor.clone(&host1.root));

        descriptor.safely_drop(&host1.root);
    }
    hosts.insert(0, host1);

    // Process hosts in a worker thread
    let worker = thread::spawn(move || {
        for (host_id, host) in &mut hosts {
            // Dup a file descriptor. We clone RootedRc without any additional
            // atomic operations; it's protected by the host lock.
            let descriptor = host.processes[&0].descriptors[&0].clone(&host.root);
            host.processes
                .get_mut(&0)
                .unwrap()
                .descriptors
                .insert(2, descriptor);

            // Iterate
            for (pid, process) in &host.processes {
                for (fid, descriptor) in &process.descriptors {
                    println!(
                        "host_id:{} pid:{} fid:{} open:{}",
                        host_id, pid, fid, descriptor.open
                    );
                }
            }
        }
        hosts
    });

    // Wait for worker to finish and get hosts back.
    let _hosts = worker.join().unwrap();
    println!("worker done as expected");
}

// For `cargo test --examples`
#[test]
fn test() {
    main();
}
