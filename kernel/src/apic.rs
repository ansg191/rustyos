use core::{
    arch::asm,
    ops::{Deref, DerefMut},
};

use spin::{Lazy, Mutex};
use x86::apic::{ioapic::IoApic, xapic::XAPIC};

use crate::{
    memory::PHYSICAL_MEM_START,
    pit::{OperatingMode, PIT0},
};

const LAPIC_PHYS_ADDR: u64 = 0xfee0_0000;

pub static LAPIC: Lazy<Mutex<XAPIC>> = Lazy::new(|| {
    disable_8259();

    let apic_region = unsafe {
        core::slice::from_raw_parts_mut(
            (PHYSICAL_MEM_START + LAPIC_PHYS_ADDR).as_mut_ptr(),
            0x1000 / 4,
        )
    };
    Mutex::new(XAPIC::new(apic_region))
});

pub static IOAPIC: Lazy<Mutex<IoApicWrapper>> = Lazy::new(|| {
    let acpi = crate::acpi::get_acpi().expect("ACPI tables should be available");
    let platform = acpi
        .platform_info()
        .expect("ACPI should provide platform info");
    let acpi::InterruptModel::Apic(apic) = platform.interrupt_model else {
        panic!("Interrupt model should be APIC");
    };

    let phys_addr = apic.io_apics[0].address;
    let virt_addr = PHYSICAL_MEM_START + u64::from(phys_addr);

    Mutex::new(IoApicWrapper(unsafe {
        IoApic::new(virt_addr.as_u64() as usize)
    }))
});

#[repr(transparent)]
pub struct IoApicWrapper(IoApic);

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for IoApicWrapper {}

impl Deref for IoApicWrapper {
    type Target = IoApic;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for IoApicWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Disable the 8259 PIC
fn disable_8259() {
    // https://wiki.osdev.org/PIC#Disabling
    unsafe {
        asm!(
            "mov al, 0xff",
            "out 0xa1, al",
            "out 0x21, al",
            options(nostack, nomem)
        );
    }
}

pub static CPU_FREQ: Lazy<u64> = Lazy::new(calc_cpu_freq);

/// Calculate the CPU clock frequency per second
fn calc_cpu_freq() -> u64 {
    x86_64::instructions::interrupts::without_interrupts(|| {
        // Prepare the PIT to sleep for 10ms (100 Hz)
        PIT0.start_timer(OperatingMode::InterruptOnTerminalCount, 100)
            .unwrap();

        let start_tsc = unsafe { x86::time::rdtsc() };

        // Wait for the PIT to reach 0
        while PIT0.get_count() != 0 {}

        let end_tsc = unsafe { x86::time::rdtsc() };

        // Calculate the CPU frequency
        let cycles_per_10ms = end_tsc - start_tsc;
        cycles_per_10ms * 100
    })
}
