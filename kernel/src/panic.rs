use x86_64::instructions::{hlt, interrupts};

use crate::kprintln;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Disable interrupts
    interrupts::disable();

    // Print panic message
    kprintln!("KERNEL PANIC:");
    kprintln!("{}", info);

    // Halts forever
    loop {
        hlt();
    }
}

pub fn halt_and_never_return() -> ! {
    // Disable interrupts
    interrupts::disable();

    // Halt forever
    loop {
        hlt();
    }
}
