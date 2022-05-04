use crate::fs::FileOp;
use std::fs::File;
use std::io;
use std::io::Error;
use std::time::Duration;

pub enum Op {
    None,
    AddPriority(i32),
    SubPriority(i32),
    SetPriority(i32),
    FileOp(FileOp),
}

#[derive(Debug)]
pub enum OpsRes {
    Empty,
    FileReadRes(FileReadRes),
    FileWriteRes(FileWriteRes),
}

impl Default for OpsRes {
    fn default() -> Self {
        OpsRes::Empty
    }
}

#[derive(Debug)]
pub enum FileReadRes {
    Ok(usize),
    FileBufReturnNone,
    Err(Error),
}

#[derive(Debug)]
pub enum FileWriteRes {
    Ok { path: String },
    Err { path: String, err: Error },
}

#[derive(Debug)]
pub enum Stone {
    Time(Duration),
    Ops(OpsType),
}

#[derive(Debug)]
pub enum OpsType {
    File,
    Net,
}
