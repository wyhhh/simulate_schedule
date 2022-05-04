use crate::fs::FileOp;
use crate::fs::TXTS;
use crate::ops::Op;
use crate::ops::OpsRes;
use crate::pcb::PInfo;
use crate::pcb::PollRes;
use crate::pcb::Process;
use crossbeam_channel::Sender;
use rg::extend::Case;
use rg::fmt::json::Json;
use rg::Mode;
use rg::Rg;
use std::borrow::Cow;
use std::fmt::Write;
use std::ops::Range;
use std::ops::RangeInclusive;
use std::path::Path;
use std::thread;
use std::time::Duration;
use wutil::convert::static_ref_mut;
use wutil::convert::StaticRef;
use wutil::convert::StaticRefArray;
use wutil::random::gen;
use wutil::types::SStr;
use wutil::types::SS;

#[macro_export]
macro_rules! process {
	($struct: ident, $name: expr, $($state:expr);*, $({ $($state_code:tt)+ });*, { $($ready_code:tt)+ }) => {
		#[derive(Debug)]
		pub struct $struct(i32, String);

		impl $struct {
			pub fn new() -> Self {
				Self(0, $name.to_string())
			}
		}

		impl Process for $struct {
			fn name(&self) -> &String {
				&self.1
			}

			fn poll(&mut self, _: Sender<SStr>, _ops_res: OpsRes) -> $crate::processes::PollRes {
				use $crate::processes::PollRes;

				self.0 += 1;

				match self.0 {
					$(
						$state => {
							$($state_code)+
						}
					)*
					_ => {
						$($ready_code)+
					}
				}
			}
		}
	}
}

process!(P2, "P2",,, {PollRes::Ready});
process!(P3, "P3", 1;2,
{

    thread::sleep(Duration::from_millis(100));
    PollRes::Polling(Op::None)
};
{

    thread::sleep(Duration::from_millis(50));
    PollRes::Polling(Op::None)
},
{PollRes::Ready}
);
process!(P4, "P4", 1;2;3,
{

    thread::sleep(Duration::from_millis(100));
    PollRes::Polling(Op::None)
};
{

    thread::sleep(Duration::from_millis(100));
    PollRes::Polling(Op::None)
};
{

    thread::sleep(Duration::from_millis(2000));
    PollRes::Polling(Op::None)
},
{PollRes::Ready}
);
process!(P5, "P5", 1;2;3;4,
{

    thread::sleep(Duration::from_millis(100));
    PollRes::Polling(Op::None)
};
{

    thread::sleep(Duration::from_millis(100));
    PollRes::Polling(Op::None)
};
{

    thread::sleep(Duration::from_millis(2000));
    PollRes::Polling(Op::None)
};
{

    thread::sleep(Duration::from_millis(400));
    PollRes::Polling(Op::None)
},
{PollRes::Ready}
);

#[derive(Debug)]
pub struct P1 {
    n: i32,
    name: String,
}

impl P1 {
    pub fn new() -> Self {
        Self {
            n: 0,
            name: "P1".to_string(),
        }
    }
}

impl Process for P1 {
    fn name(&self) -> &String {
        &self.name
    }

    fn poll(&mut self, s: Sender<SStr>, ops_res: OpsRes) -> crate::pcb::PollRes {
        self.n += 1;

        match self.n {
            1 => {
                thread::sleep(Duration::from_millis(40));
                PollRes::Polling(Op::AddPriority(10))
            }
            2 => {
                thread::sleep(Duration::from_millis(30));
                PollRes::Polling(Op::FileOp(FileOp::Read(SS::SPath(Cow::Borrowed(
                    Path::new("file_open"),
                )))))
            }
            3 => {
                thread::sleep(Duration::from_millis(30));
                PollRes::Polling(Op::None)
            }
            _ => PollRes::Ready,
        }
    }
}

#[derive(Debug)]
pub struct RandomProcess {
    state: u32,
    state_max: u32,
    sleep_range: RangeInclusive<Duration>,
    name: String,
    rg: Rg<'static>,
    buf: String,
    json: Json,
}

impl RandomProcess {
    pub fn new(state_max: u32, sleep_range: RangeInclusive<Duration>) -> Self {
        Self {
            state_max,
            sleep_range,
            name: "Random Ready".to_string(),
            state: 0,
            rg: Rg::new(),
            buf: String::new(),
            json: Json::new(),
        }
    }
    fn write_name(&mut self, cur_sleep_range: Duration) {
        self.name.clear();
        write!(
            &mut self.name,
            "[{}-{:.0?} {}]",
            self.state + 1,
            cur_sleep_range,
            self.state_max,
        );
    }
}

impl Process for RandomProcess {
    fn name(&self) -> &String {
        &self.name
    }

    fn poll(&mut self, msg_tx: Sender<SStr>, ops_res: OpsRes) -> PollRes {
        match ops_res {
            OpsRes::Empty => {}
            OpsRes::FileReadRes(r) => match r {
                crate::ops::FileReadRes::Ok(n) => {
                    msg_tx.send(Cow::Owned(format!("{} READ: {}", self.name, self.buf)));
                }
                crate::ops::FileReadRes::FileBufReturnNone => {}
                crate::ops::FileReadRes::Err(e) => {
                    msg_tx.send(Cow::Owned(format!("{} READ ERR: {}", self.name, e)));
                }
            },
            OpsRes::FileWriteRes(w) => match w {
                crate::ops::FileWriteRes::Ok { path } => {
                    msg_tx.send(Cow::Owned(format!("{} [{}] WRITE OK.", self.name, path)));
                }
                crate::ops::FileWriteRes::Err { path, err } => {
                    msg_tx.send(Cow::Owned(format!(
                        "{} [{path}] WRITE ERR: {}",
                        self.name, err
                    )));
                }
            },
        }

        if self.state == self.state_max {
            return PollRes::Ready;
        }
        let cur_sleep_range = gen(self.sleep_range.clone());

        self.write_name(cur_sleep_range);

        /* probably send a msg to print */
        let print_choice = gen(0..10000_i32);

        if print_choice == 0 {
            msg_tx.send(Cow::Owned(format!(
                "{} say: {}",
                self.name,
                self.rg.once::<&str, _>(Mode::ASLP(","))
            )));
        }

        /* sleep for heavy work */
        thread::sleep(cur_sleep_range);

        self.state += 1;

        let op_choice = gen(0..10000_i32);
        match op_choice {
            0 => PollRes::Polling(Op::AddPriority(gen(1..=30))),
            1 => PollRes::Polling(Op::SubPriority(gen(1..=30))),
            2 => PollRes::Polling(Op::SetPriority(gen(1..=30))),
            3 => PollRes::Polling(Op::FileOp(FileOp::Read({
                unsafe {
                    let choice = gen(0..TXTS.len() + 2);

                    if choice < TXTS.len() {
                        SS::SPath(Cow::Borrowed(TXTS.get_unchecked(choice)))
                    } else {
                        SS::SStr(self.rg.once::<&str, _>(Mode::ASLP(",")))
                    }
                }
            }))),
            4 => PollRes::Polling(Op::FileOp(FileOp::Write {
                path: format!("out/{}.txt", self.rg.word(3..=10, Case::Lower)),
                content: {
                    match gen(0..10) {
                        0 => Cow::Owned(self.json.generate()),
                        _ => self.rg.once::<&str, _>(Mode::ASLP(",")),
                    }
                },
            })),
            _ => PollRes::Polling(Op::None),
        }
    }

    fn file_buf(&mut self) -> Option<&mut String> {
        Some(&mut self.buf)
    }
}

pub struct RandomFactory {
    vec: Vec<RandomProcess>,
    idx: usize,
}

impl RandomFactory {
    pub fn new(size: usize) -> Self {
        let mut vec = Vec::with_capacity(size);

        for _ in 0..size {
            vec.push(RandomProcess::new(
                gen(0..=10000),
                gen(Duration::from_millis(0)..=Duration::from_millis(15))
                    ..=gen(Duration::from_millis(15)..Duration::from_millis(30)),
            ));
        }
        Self { vec, idx: 0 }
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }
}

impl Iterator for &'_ mut RandomFactory {
    type Item = &'static mut RandomProcess;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx == self.vec.len() {
            return None;
        }
        unsafe {
            let ret = self.vec.get_unchecked_mut(self.idx).static_ref_mut();

            self.idx += 1;

            Some(ret)
        }
    }
}

#[test]
fn test() {}
