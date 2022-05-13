use objgraph::{Root, RootedRc};
use std::{
    collections::HashMap,
    thread::{self, Thread},
};

struct Host {
    processes: HashMap<u32, Process>,
}

struct Process {
    descriptors: HashMap<u32, RootedRc<Descriptor>>,
}

struct Descriptor {
    open: bool,
}

pub fn main() {
    let mut hosts = HashMap::<u32, Root<Host>>::new();

    // host1 has 2 processes, which have a shared Descriptor.
    // (Maybe one was forked from the other)
    let host1 = Root::new(Host {
        processes: HashMap::new(),
    });
    {
        let mut host1_lock = host1.lock();
        let descriptor = RootedRc::new(host1.tag(), Descriptor { open: true });

        // Process 0 has a reference to the descriptor.
        host1_lock.processes.insert(
            0,
            Process {
                descriptors: HashMap::new(),
            },
        );
        host1_lock
            .processes
            .get_mut(&0)
            .unwrap()
            .descriptors
            .insert(0, descriptor.clone());

        // So does Process 1.
        host1_lock.processes.insert(
            1,
            Process {
                descriptors: HashMap::new(),
            },
        );
        host1_lock
            .processes
            .get_mut(&1)
            .unwrap()
            .descriptors
            .insert(0, descriptor.clone());
    }
    hosts.insert(0, host1);

    // Process hosts in a worker thread
    let worker = thread::spawn(move || {
        for (host_id, host) in &hosts {
            let mut lock = host.lock();
            // Dup a file descriptor. We clone RootedRc without any additional
            // atomic operations; it's protected by the host lock.
            let descriptor = lock.processes[&0].descriptors[&0].clone();
            lock.processes
                .get_mut(&0)
                .unwrap()
                .descriptors
                .insert(2, descriptor);

            // Iterate
            for (pid, process) in &lock.processes {
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
    let hosts = worker.join().unwrap();
    println!("worker done as expected");

    // Uncomment to see a panic.
    /*
    // While a RootedRc can "escape", we'll get a panic if the reference count
    // is manipulated without the corresponding host lock being held.
    let escaped_descriptor = {
        let lock = hosts[&0].lock();
        lock.processes[&0].descriptors[&0].clone()
    };

    println!("We now own an escaped descriptor. We will next panic because of dropping it without the lock held.");
    drop(escaped_descriptor);
    */
}
