use lazy_static::lazy_static;
use x86::apic::ApicControl;
use x86_64::{
    set_general_handler,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
};

use crate::kprintln;

pub const IRQ0: u8 = 0x20;
pub const IRQ_COM1: u8 = 4;

#[inline]
fn ack_lapic() {
    crate::apic::LAPIC.lock().eoi();
}

fn general_handler(_: InterruptStackFrame, idx: u8, errcode: Option<u64>) {
    kprintln!("Interrupt!:");
    kprintln!("\tidx: {:x}", idx);
    kprintln!("\terrcode: {:?}", errcode);
    panic!("Interrupt!");
}

extern "x86-interrupt" fn timer_handler(_: InterruptStackFrame) {
    crate::time::TICKS.inc();
    ack_lapic();
}

extern "x86-interrupt" fn com1_handler(_: InterruptStackFrame) {
    crate::serial::COM1.lock().handle_interrupt();
    ack_lapic();
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
        idt[IRQ0.into()].set_handler_fn(timer_handler);
        idt[(IRQ0 + IRQ_COM1).into()].set_handler_fn(com1_handler);
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt
    };
}

pub fn init_idt() {
    // Load the IDT
    IDT.load();
}
