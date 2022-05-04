use crate::ops::Op;
use crate::ops::OpsRes;
use crate::ops::Stone;
use crate::scheduler::Share;
use core::fmt;
use crossbeam_channel::Receiver;
use crossbeam_channel::Sender;
use parking_lot::Mutex;
use std::collections::LinkedList;
use std::mem::take;
use std::time::Duration;
use wutil::convert;
use wutil::convert::make_mut;
use wutil::convert::StaticRef;
use wutil::types::SStr;
use wutil::util::time_test;

pub const INIT_PRIORITY: i32 = 0;

pub enum PollRes {
    Polling(Op),
    Ready,
}

pub trait Process: fmt::Debug + Send + Sync {
    fn name(&self) -> &String;
    fn poll(&mut self, msg_tx: Sender<SStr>, ops_res: OpsRes) -> PollRes;
    fn file_buf(&mut self) -> Option<&mut String> {
        None
    }
}

#[derive(Debug)]
pub struct Pcb {
    pub p: &'static mut dyn Process,
    pub pinfo: &'static PInfo,
    pub ops_res: OpsRes,
    pub msg_tx: Sender<SStr>,
}

#[derive(Debug, Default)]
pub struct PInfo {
    pub id: u32,
    pub metric: Metric,
    pub name: Option<&'static String>,
    pub run_slices: f32,
    pub done: bool,
    pub stones: LinkedList<Stone>,
}

const EACH_COMPENSATE: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Copy, Default)]
pub struct Metric {
    pub priority: i32,
    pub running_time: Duration,
}

impl Metric {
    fn value(&self) -> f32 {
        self.running_time.as_secs_f32() - self.priority as f32 * EACH_COMPENSATE.as_secs_f32()
    }
}

impl PartialEq for Metric {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.running_time == other.running_time
        // self.value() == other.value()
    }
}

impl PartialEq for Pcb {
    fn eq(&self, other: &Self) -> bool {
        self.pinfo.metric == other.pinfo.metric
    }
}

impl Eq for Metric {}
impl Eq for Pcb {}

impl PartialOrd for Metric {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.value().partial_cmp(&self.value())
    }
}

impl PartialOrd for Pcb {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.pinfo.metric.cmp(&other.pinfo.metric))
    }
}

impl Ord for Pcb {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.pinfo.metric.cmp(&other.pinfo.metric)
    }
}

impl Ord for Metric {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        unsafe { self.partial_cmp(other).unwrap_unchecked() }
    }
}

impl PInfo {
    pub fn new() -> Self {
        Self {
            id: 0,
            name: None,
            run_slices: 0.0,
            done: false,
            metric: Metric {
                priority: INIT_PRIORITY,
                running_time: Duration::ZERO,
            },
            stones: LinkedList::new(),
        }
    }
}

impl Pcb {
    pub fn new(
        id: u32,
        p: &'static dyn Process,
        msg_tx: Sender<SStr>,
        pinfo: &'static PInfo,
    ) -> Self {
        // Safety: here in main thread single thread.
        let pinfo = unsafe { make_mut(pinfo) };
        pinfo.id = id;
        pinfo.name = Some(p.name());

        Self {
            p: unsafe { make_mut(p) },
            msg_tx,
            pinfo,
            ops_res: OpsRes::Empty,
        }
    }

    fn pinfo_mut(&self) -> &mut PInfo {
        // Safety: the process is unique! So is safe
        unsafe { make_mut(&self.pinfo) }
    }

    pub fn done(&self) {
        // Safety: the process is unique! So is safe
        self.pinfo_mut().done = true;
    }

    pub fn poll_wrap(&mut self, time_slice: Duration) -> PollRes {
        let ops_res = take(&mut self.ops_res);
        // Safety: the process is unique! So is safe
        let (time, poll_res) =
            time_test(|| unsafe { convert::make_mut(self).p.poll(self.msg_tx.clone(), ops_res) });
        let pinfo = self.pinfo_mut();
        let slices = time.div_duration_f32(time_slice);

        pinfo.name = unsafe { Some(self.p.name().static_ref()) };
        pinfo.run_slices += slices;
        pinfo.metric.running_time += time;

        let need_push = match unsafe { make_mut(self.pinfo) }.stones.back_mut() {
            Some(stone) => match stone {
                Stone::Time(d) => {
                    *d += time;
                    false
                }
                Stone::Ops(_o) => true,
            },
            None => true,
        };

        if need_push {
            unsafe { make_mut(self.pinfo) }
                .stones
                .push_back(Stone::Time(time));
        }

        poll_res
    }
}
