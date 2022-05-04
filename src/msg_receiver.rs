use crossbeam_channel::Receiver;
use std::io;
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use wutil::types::SStr;

pub struct MsgReceiver {
    rx: Receiver<SStr>,
    msg_done: &'static AtomicBool,
}

impl MsgReceiver {
    pub fn new(rx: Receiver<SStr>, msg_done: &'static AtomicBool) -> io::Result<Self> {
        Ok(Self { rx, msg_done })
    }

    pub fn start(&mut self, addr: &str) -> io::Result<()> {
        for s in &self.rx {
            let mut stream = TcpStream::connect(addr)?;
            stream.write(s.as_bytes())?;
        }

        println!("{:?}", 222);
        self.msg_done.store(true, Ordering::Relaxed);

        Ok(())
    }
}
