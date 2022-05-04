#![feature(div_duration)]

use crate::pcb::PInfo;
use crate::pcb::Pcb;
use crate::printer::Printer;
use crate::scheduler::Share;
use crossbeam::sync::Parker;
use crossbeam_channel::unbounded;
use crossbeam_channel::Receiver;
use crossbeam_epoch::default_collector;
use crossbeam_epoch::pin;
use crossbeam_skiplist_piedb::SkipList;
use msg_receiver::MsgReceiver;
use parking_lot::Condvar;
use parking_lot::Mutex;
use pcb::Process;
use processes::*;
use scheduler::SchedulerBuilder;
use std::collections::LinkedList;
use std::env;
use std::hash::BuildHasherDefault;
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use work_queue::Queue;
use wutil::convert::make_mut;
use wutil::convert::StaticRef;
use wutil::convert::StaticRefArray;
use wutil::static_refs;
use wutil::types::SStr;

mod fs;
mod msg_receiver;
mod ops;
mod pcb;
mod printer;
mod processes;
mod scheduler;
mod worker_info;

macro_rules! make_pcbs {
	($($p:ident, $pinfo:ident, $t:ident;)+) => {
		wutil::static_refs_mut! {
			$(
				$p = $t::new();
				$pinfo = PInfo::new();
			)+
		}
	}
}

const RANDOM_PROCESSES: usize = 20;

fn main() -> io::Result<()> {
    let threads = num_cpus::get();
    // let threads = 3;
    static_refs! {
        global_queue = Queue::new(threads as usize, 32);
        share = Share::new();
        printer = Printer::new(share);
        worker_infos = LinkedList::new();
        msg_done = AtomicBool::new(false);
        fs_unparkers = Vec::with_capacity(threads);
    };

    let msg_addr = env::args().nth(1).unwrap();
    let (msg_tx, msg_rx) = unbounded();
    start_assitor(msg_rx, msg_addr, msg_done)?;

    // make pcbs
    make_pcbs! {
        p1,p1info,P1;
        p2,p2info,P2;
        p3,p3info,P3;
        p4,p4info,P4;
        p5,p5info,P5;
    };

    // make parkers
    let parkers = StaticRefArray::new(threads as usize, || {
        let parker = Parker::new();
        let unparker = parker.unparker().clone();
        unsafe { make_mut(fs_unparkers) }.push(unparker.clone());

        (parker, unparker)
    });

    // Safety: The life of it go along with the scheduler
    let parkers = unsafe { (&parkers).static_ref() };

    let mut s = SchedulerBuilder::new().build(
        threads,
        global_queue,
        printer,
        share,
        worker_infos,
        parkers,
        msg_done,
        fs_unparkers,
        msg_tx.clone(),
    );
    s.execute(p1, msg_tx.clone(), p1info);
    s.execute(p2, msg_tx.clone(), p2info);
    s.execute(p3, msg_tx.clone(), p3info);
    s.execute(p4, msg_tx.clone(), p4info);
    s.execute(p5, msg_tx.clone(), p5info);

    // We use factory to create any amount random processes
    let mut factory = RandomFactory::new(RANDOM_PROCESSES);
    let pinfos = StaticRefArray::new(factory.len(), Default::default);

    for (r, pinfo) in &mut factory.zip(pinfos.iter()) {
        s.execute(r, msg_tx.clone(), pinfo);
    }

    drop(msg_tx);
    // MUST call one of below, or be memory of out bounds!
    // s.infinite_run();
    s.join();
    Ok(())
}

fn start_assitor(
    msg_rx: Receiver<SStr>,
    addr: String,
    msg_done: &'static AtomicBool,
) -> io::Result<()> {
    let mut r = MsgReceiver::new(msg_rx, msg_done)?;

    thread::spawn(move || {
        r.start(&addr);
    });
    Ok(())
}
