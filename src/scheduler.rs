use crate::fs::fs_run;
use crate::fs::init_txts;
use crate::ops::Op;
use crate::pcb::PInfo;
use crate::pcb::Pcb;
use crate::pcb::PollRes;
use crate::pcb::Process;
use crate::pcb::INIT_PRIORITY;
use crate::printer;
use crate::printer::Printer;
use crate::worker_info::WorkerInfo;
use crossbeam::channel::unbounded;
use crossbeam::channel::Receiver;
use crossbeam::sync::Parker;
use crossbeam::sync::Unparker;
use crossbeam_channel::Sender;
use std::borrow::Cow;
use std::collections::BinaryHeap;
use std::collections::LinkedList;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;
use work_queue::Queue;
use wutil::convert::make_mut;
use wutil::convert::StaticRef;
use wutil::convert::StaticRefArray;
use wutil::random::gen;
use wutil::types::SStr;
use wutil::util::time_test;

pub struct SchedulerBuilder {
    time_slice: Duration,
    print: bool,
    print_interval: Duration,
}

impl SchedulerBuilder {
    pub fn new() -> Self {
        Self {
            time_slice: Duration::from_millis(20),
            print: true,
            print_interval: Duration::from_millis(200),
        }
    }

    pub fn time_slice(mut self, time_slice: Duration) -> Self {
        self.time_slice = time_slice;
        self
    }

    pub fn print(mut self, print: bool) -> Self {
        self.print = print;
        self
    }

    pub fn print_interval(mut self, d: Duration) -> Self {
        self.print_interval = d;
        self
    }

    pub fn build(
        self,
        threads: usize,
        global_queue: &'static Queue<Pcb>,
        printer: &'static Printer,
        share: &'static Share,
        worker_infos: &'static LinkedList<WorkerInfo>,
        parkers: &'static StaticRefArray<(Parker, Unparker)>,
        msg_done: &'static AtomicBool,
        fs_unparkers: &'static Vec<Unparker>,
        msg_tx: Sender<SStr>,
    ) -> Scheduler {
        // if print, start the print thread
        if self.print {
            // Safety: here in main thread single thread.
            let printer = unsafe { make_mut(printer) };
            printer.schedule_threads = threads;
            printer.worker_infos = Some(worker_infos);
            thread::spawn(move || printer::print(printer, self.print_interval));
        }

        let mut workers = Vec::with_capacity(threads as usize);
        let mut local_queues = global_queue.local_queues();
        let (shutdown_tx, shutdown_rx) = unbounded();
        let mut parkers_iter = parkers.iter();
        let mut unparkers_vec = Vec::with_capacity(threads);

        for id in 0..threads {
            let mut unparkers = Vec::with_capacity(threads - 1);
            for (idx, (_, unparker)) in parkers.iter().enumerate() {
                if id != idx {
                    unparkers.push(unparker.clone());
                }
            }
            unparkers_vec.push(unparkers);
        }

        let (fs_tx, fs_rx) = unbounded();

        for _ in 0..threads / 2 + 1 {
            init_txts();
            let rx = fs_rx.clone();
            thread::spawn(move || fs_run(rx, global_queue, fs_unparkers));
        }

        let mut unparkers_vec = unparkers_vec.into_iter();

        for id in 0..threads {
            let msg_tx = msg_tx.clone();
            let fs_tx = fs_tx.clone();
            let (parker, unparker) = parkers_iter.next().unwrap();
            let unparkers = unparkers_vec.next().unwrap();

            let mut local_queue = local_queues.next().unwrap();
            let shutdown_tx = shutdown_tx.clone();
            let worker_info = WorkerInfo {
                id,
                start_point: Instant::now(),
                waiting_time: Duration::ZERO,
                idle: false,
            };

            let worker_info_ref = unsafe {
                make_mut(worker_infos).push_back(worker_info);
                worker_infos.back().unwrap().static_ref_mut()
            };

            let h = std::thread::spawn(move || {
                let mut priority_queue = BinaryHeap::new();

                loop {
                    let processes = share.processes.load(Ordering::Relaxed);
                    let done = share.done.load(Ordering::Relaxed);
                    let remain_processes = processes - done;
                    let avg_processes = remain_processes / threads;
                    let mut cnt = 0;

                    while let Some(pcb) = local_queue.pop() {
                        priority_queue.push(pcb);
                        cnt += 1;

                        if cnt >= avg_processes {
                            break;
                        }
                    }

                    if priority_queue.is_empty() {
                        worker_info_ref.idle = true;
                        let (time, ()) = time_test(|| parker.park());

                        if unsafe { WORKER_RETURN } {
                            return;
                        }

                        worker_info_ref.waiting_time += time;
                        worker_info_ref.idle = false;
                    } else {
                        while let Some(mut pcb) = priority_queue.pop() {
                            match pcb.poll_wrap(self.time_slice) {
                                PollRes::Polling(op) => {
                                    match op {
                                        Op::None => {}
                                        Op::FileOp(file_op) => {
                                            fs_tx.send((pcb, file_op));
                                            continue;
                                        }
                                        Op::AddPriority(p) => {
                                            unsafe { pcb.pinfo.static_ref_mut() }
                                                .metric
                                                .priority += p;

                                            msg_tx.send(Cow::Owned(format!(
                                                "{} ADD PRIORITY => {}",
                                                pcb.p.name(),
                                                p
                                            )));
                                        }
                                        Op::SubPriority(p) => {
                                            unsafe { pcb.pinfo.static_ref_mut() }
                                                .metric
                                                .priority -= p;
                                            msg_tx.send(Cow::Owned(format!(
                                                "{} SUB PRIORITY => {}",
                                                pcb.p.name(),
                                                p
                                            )));
                                        }
                                        Op::SetPriority(p) => {
                                            unsafe { pcb.pinfo.static_ref_mut() }.metric.priority =
                                                p;
                                            msg_tx.send(Cow::Owned(format!(
                                                "{} SET PRIORITY => {}",
                                                pcb.p.name(),
                                                p
                                            )));
                                        }
                                    }

                                    let choice = gen(0..threads + 1);

                                    if choice == 0 {
                                        local_queue.global().push(pcb);
                                    } else {
                                        local_queue.push(pcb);
                                    }

                                    let maybe_awaken = gen(0..unparkers.len());
                                    unsafe {
                                        unparkers.get_unchecked(maybe_awaken).unpark();
                                    }
                                }
                                PollRes::Ready => {
                                    pcb.done();

                                    let done = share.done.fetch_add(1, Ordering::Relaxed);

                                    if done + 1 == processes {
                                        shutdown_tx.send(processes);
                                    }
                                }
                            }
                        }
                    }
                }
            });

            workers.push(Worker {
                handle: h,
                unparker: unparker.clone(),
            });
        }

        drop(fs_tx);
        // drop(msg_tx);

        Scheduler {
            workers,
            printer,
            print: self.print,
            next_id: 1,
            share,
            ready_queue: global_queue,
            shutdown: shutdown_rx,
            msg_done,
        }
    }
}

static mut WORKER_RETURN: bool = false;

pub struct Scheduler {
    ready_queue: &'static Queue<Pcb>,
    share: &'static Share,
    workers: Vec<Worker>,
    printer: &'static Printer,
    print: bool,
    next_id: u32,
    shutdown: Receiver<usize>,
    msg_done: &'static AtomicBool,
}

/// The share data between schedule threads and main thread of `Scheduler`.
pub struct Share {
    pub processes: AtomicUsize,
    pub done: AtomicUsize,
    pub printer_done: AtomicBool,
    pub scheduler_done: AtomicBool,
}

struct Worker {
    handle: JoinHandle<()>,
    unparker: Unparker,
}

impl Share {
    pub fn new() -> Self {
        Self {
            processes: AtomicUsize::new(0),
            done: AtomicUsize::new(0),
            printer_done: AtomicBool::new(false),
            scheduler_done: AtomicBool::new(false),
        }
    }
}

// 1. Self starts
// 2. Ready for new process && running process which can run
impl Scheduler {
    pub fn execute(
        &mut self,
        p: &'static mut (dyn Process + Send + Sync),
        msg_rx: Sender<SStr>,
        pinfo: &'static PInfo,
    ) {
        self.execute_priority(p, msg_rx, pinfo, INIT_PRIORITY);
    }

    pub fn execute_priority(
        &mut self,
        p: &'static mut dyn Process,
        msg_tx: Sender<SStr>,
        pinfo: &'static PInfo,
        priority: i32,
    ) {
        // Safety: here in main thread single thread.
        let pinfo = unsafe { make_mut(pinfo) };
        pinfo.metric.priority = priority;

        self.ready_queue
            .push(Pcb::new(self.next_id, p, msg_tx, pinfo));

        unsafe { make_mut(self.printer) }.pinfos.push_back(pinfo);
        self.share.processes.fetch_add(1, Ordering::Relaxed);
        self.notify_all();
        self.next_id += 1;
    }

    pub fn infinite_run(self) {
        for w in self.workers {
            w.handle.join().unwrap();
        }
    }

    pub fn join(self) {
        while let Ok(processes) = self.shutdown.recv() {
            if processes == self.share.processes.load(Ordering::Relaxed) {
                break;
            }
        }

        unsafe {
            WORKER_RETURN = true;
        }
        self.notify_all();

        self.share.scheduler_done.store(true, Ordering::Relaxed);

        /* wait for printer && another msg console working done! */
        while !self.share.printer_done.load(Ordering::Relaxed) {}
        while !self.msg_done.load(Ordering::Relaxed) {}
    }

    fn notify_all(&self) {
        for w in &self.workers {
            w.unparker.unpark();
        }
    }
}
