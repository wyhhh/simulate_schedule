use crate::ops::FileReadRes;
use crate::ops::FileWriteRes;
use crate::ops::OpsRes;
use crate::ops::OpsType;
use crate::ops::Stone;
use crate::pcb::Pcb;
use crossbeam::sync::Unparker;
use crossbeam_channel::Receiver;
use std::fs::read_dir;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use work_queue::Queue;
use wutil::convert::make_mut;
use wutil::random::gen;
use wutil::types::SPath;
use wutil::types::SStr;
use wutil::types::SS;

pub static mut TXTS: Vec<PathBuf> = Vec::new();

pub enum FileOp {
    Read(SS),
    Write { path: String, content: SStr },
}

pub fn init_txts() {
    for entry in read_dir("./txt").unwrap() {
        unsafe {
            TXTS.push(entry.unwrap().path());
        }
    }
}

pub fn fs_run(
    rx: Receiver<(Pcb, FileOp)>,
    ready_queue: &'static Queue<Pcb>,
    fs_unparkers: &'static Vec<Unparker>,
) {
    for (mut pcb, file_op) in rx {
        let stone = match file_op {
            FileOp::Read(p) => {
                let file = File::open(p);

                pcb.ops_res = match pcb.p.file_buf() {
                    Some(buf) => match file {
                        Ok(mut f) => {
                            buf.clear();

                            match f.read_to_string(buf) {
                                Ok(n) => OpsRes::FileReadRes(FileReadRes::Ok(n)),
                                Err(e) => OpsRes::FileReadRes(FileReadRes::Err(e)),
                            }
                        }
                        Err(e) => OpsRes::FileReadRes(FileReadRes::Err(e)),
                    },
                    None => OpsRes::FileReadRes(FileReadRes::FileBufReturnNone),
                };

                Stone::Ops(OpsType::File)
            }
            FileOp::Write { path, content } => {
                let file = File::create(&path);

                pcb.ops_res = match file {
                    Ok(mut f) => match f.write_all(content.as_bytes()) {
                        Ok(()) => OpsRes::FileWriteRes(FileWriteRes::Ok { path }),
                        Err(e) => OpsRes::FileWriteRes(FileWriteRes::Err { path, err: e }),
                    },
                    Err(e) => OpsRes::FileWriteRes(FileWriteRes::Err { path, err: e }),
                };

                Stone::Ops(OpsType::File)
            }
        };

        unsafe { make_mut(pcb.pinfo) }.stones.push_back(stone);
        ready_queue.push(pcb);

        let maybe_awaken = gen(0..fs_unparkers.len());
        unsafe {
            fs_unparkers.get_unchecked(maybe_awaken).unpark();
        }
    }
}
