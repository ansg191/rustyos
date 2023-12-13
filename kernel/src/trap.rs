use lazy_static::lazy_static;
use x86_64::{
    set_general_handler,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
};

use crate::kprintln;

fn general_handler(_: InterruptStackFrame, idx: u8, errcode: Option<u64>) {
    kprintln!("Interrupt!:");
    kprintln!("\tidx: {:x}", idx);
    kprintln!("\terrcode: {:?}", errcode);
    panic!("Interrupt!");
}

extern "x86-interrupt" fn irq_handler(_: InterruptStackFrame) {
    if let Some(ref mut lapic) = *crate::lapic::LAPIC.lock() {
        lapic.end_of_interrupt();
    }
}

extern "x86-interrupt" fn page_fault_handler(_: InterruptStackFrame, errcode: PageFaultErrorCode) {
    kprintln!("Page fault!");
    kprintln!("\terr code: {:?}", errcode);
    kprintln!(
        "\taddress accessed: {:x}",
        x86_64::registers::control::Cr2::read().as_u64()
    );
    panic!("Page fault!");
}

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        set_general_handler!(&mut idt, general_handler);
        idt[32].set_handler_fn(irq_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt
    };
}

#[no_mangle]
pub fn init_idt() {
    // Load the IDT
    IDT.load();
}
