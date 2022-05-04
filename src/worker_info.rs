use std::time::Duration;
use std::time::Instant;

#[derive(Debug)]
pub struct WorkerInfo {
    pub id: usize,
    pub start_point: Instant,
    pub waiting_time: Duration,
    pub idle: bool,
}
