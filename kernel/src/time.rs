use core::sync::atomic::{AtomicU64, Ordering};

use x86::apic::xapic::ApicRegister;
use x86_64::instructions::interrupts::without_interrupts;

use crate::{apic::LAPIC, pit::PIT0};

/// Ticks per second.
pub const TICK_FREQ: u32 = 1000;

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

pub fn start_timer() {
    without_interrupts(|| {
        let mut lapic = LAPIC.lock();

        // Tell APIC timer to use divider 16
        lapic.write(ApicRegister::XAPIC_TIMER_DIV_CONF, 0x3);

        // Prepare the PIT to sleep for 10ms (100 Hz)
        PIT0.start_timer(crate::pit::OperatingMode::InterruptOnTerminalCount, 100)
            .unwrap();

        // Set APIC init counter to -1
        lapic.write(ApicRegister::XAPIC_TIMER_INIT_COUNT, 0xffff_ffff);

        // Wait for PIT to reach 0
        while PIT0.get_count() != 0 {}

        // Stop APIC timer
        lapic.write(ApicRegister::XAPIC_LVT_TIMER, 0x10000);

        let ticks_per_10ms = 0xFFFF_FFFF - lapic.read(ApicRegister::XAPIC_TIMER_CURRENT_COUNT);
        let ticks_per_s = ticks_per_10ms * 100;

        // Start timer as periodic on IRQ 0, divider 16, with the number of ticks to achieve TICK_FREQ
        lapic.write(ApicRegister::XAPIC_LVT_TIMER, 0x20 | 0x20000);
        lapic.write(ApicRegister::XAPIC_TIMER_DIV_CONF, 0x3);
        lapic.write(
            ApicRegister::XAPIC_TIMER_INIT_COUNT,
            ticks_per_s / TICK_FREQ,
        );
    });
}
