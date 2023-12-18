use core::sync::atomic::{AtomicU64, Ordering};

/// Number of ticks since the system booted.
pub static TICKS: Ticks = Ticks::new();

#[derive(Debug)]
pub struct Ticks(AtomicU64);

impl Ticks {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
