use crate::ops::OpsType;
use crate::ops::Stone;
use crate::pcb::PInfo;
use crate::pcb::Pcb;
use crate::scheduler::Share;
use crate::worker_info::WorkerInfo;
use core::fmt;
use crossterm::cursor;
use parking_lot::Mutex;
use std::collections::LinkedList;
use std::io::stdout;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use wutil::util::rate::Rate;
use wutil::util::rate::RemainRate;

pub struct Printer {
    start_point: Instant,
    pub schedule_threads: usize,
    pub pinfos: LinkedList<&'static PInfo>,
    pub share: &'static Share,
    pub worker_infos: Option<&'static LinkedList<WorkerInfo>>,
}

impl Printer {
    pub fn new(share: &'static Share) -> Self {
        Self {
            start_point: Instant::now(),
            worker_infos: None,
            schedule_threads: 0,
            pinfos: LinkedList::new(),
            share,
        }
    }
}

pub fn print(printer: &'static Printer, d: Duration) {
    // hide the cursor
    crossterm::execute! {
        stdout(),
        cursor::Hide,
    };

    loop {
        // clear screen and move cursor at row 1 column 1
        print!("\x1B[2J\x1B[1;1H");
        println!("{}", printer);
        thread::sleep(d);
    }
}

impl fmt::Display for Printer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let done = self.share.done.load(Ordering::Relaxed);
        let processes = self.share.processes.load(Ordering::Relaxed);
        let printer_done = done == processes;

        writeln!(
            f,
            "☆ Cost Time: {:.1?} \n\
            ☆ Threads: {} \n\
			☆ Compeletes : {}/{}",
            self.start_point.elapsed(),
            self.schedule_threads,
            done,
            processes,
        );

        write!(f, "☆ Workload:")?;
        let worker_infos = self.worker_infos.unwrap();
        let mut running_threads = 0;
        for worker_info in worker_infos.iter() {
            let running_time = worker_info.start_point.elapsed();
            let remain_rate = RemainRate(
                worker_info.waiting_time.as_secs_f64(),
                running_time.as_secs_f64(),
                1,
            );
            let idle = if worker_info.idle {
                "😭"
            } else {
                running_threads += 1;
                "😁"
            };

            write!(f, "{}{}:{}", idle, worker_info.id, remain_rate)?;
        }

        writeln!(
            f,
            "\n☆ Running Threads: {}/{}",
            running_threads,
            worker_infos.len()
        )?;

        let remain_processes = processes - done;

        writeln!(
            f,
            "☆ Threads Efficiency: {}\n",
            Rate(
                running_threads as f32,
                remain_processes.min(worker_infos.len()) as f32,
                1
            )
        )?;

        for pinfo in &self.pinfos {
            // -------------each pinfo printing----------
            write!(
                f,
                "🍀 {}. {}({} {:.1?} 🍒x{:.1}): ",
                pinfo.id,
                if let Some(name) = pinfo.name {
                    name
                } else {
                    "[x]"
                },
                pinfo.metric.priority,
                pinfo.metric.running_time,
                pinfo.run_slices
            )?;

            for stone in pinfo.stones.iter() {
                match stone {
                    Stone::Time(d) => {
                        // ☀️ = 10s 🌛 = 1s ⭐️ = 50ms 🍒 = times slice
                        let x_min = d.div_duration_f32(Duration::from_secs(10)) as u32;

                        for _ in 0..x_min {
                            write!(f, "☀️")?;
                        }

                        let x_ten_sec = (d.saturating_sub(Duration::from_secs(10) * x_min))
                            .div_duration_f32(Duration::from_secs(1))
                            as u32;

                        for _ in 0..x_ten_sec {
                            write!(f, "🌛")?;
                        }

                        let x_sec = (d
                            .saturating_sub(Duration::from_secs(10) * x_min)
                            .saturating_sub(Duration::from_secs(1) * x_ten_sec))
                        .div_duration_f32(Duration::from_millis(50))
                            as u32;

                        for _ in 0..x_sec {
                            write!(f, "⭐️")?;
                        }
                    }
                    Stone::Ops(o) => match o {
                        OpsType::File => {
                            write!(f, "📃")?;
                        }
                        OpsType::Net => {
                            write!(f, "🌏")?;
                        }
                    },
                }
            }
            if pinfo.done {
                write!(f, "🤡")?;
            }
            writeln!(f)?;
        }

        if self.share.scheduler_done.load(Ordering::Relaxed) && printer_done {
            writeln!(f, "\n\nALL DONE!")?;
            self.share.printer_done.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}
