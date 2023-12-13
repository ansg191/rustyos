use core::arch::asm;

use raw_cpuid::CpuId;
use x86_64::{registers::model_specific::Msr, VirtAddr};

use crate::{
    memory::PAGE_TABLE,
    pit::{OperatingMode, PIT0},
};

pub static LAPIC: spin::Mutex<Option<Lapic>> = spin::Mutex::new(None);

const APIC_BASE_MSR: u32 = 0x1b;
const APIC_BASE_MSR_BSP: u64 = 0x100;
const APIC_BASE_MSR_ENABLE: u64 = 0x800;

pub struct Lapic {
    addr: VirtAddr,
}

impl Lapic {
    /// Initialize the local APIC
    ///
    /// Checks if the CPU supports the local APIC, and if so, enables it.
    /// Also verifies that the local APIC is working correctly.
    pub fn new() -> Result<Self, Error> {
        let cpu_id = CpuId::new();
        let has_apic = cpu_id.get_feature_info().map_or(false, |f| f.has_apic());

        if !has_apic {
            return Err(Error::NotSupported);
        }

        // Disable the 8259 PIC
        disable_8259();

        // Enable the APIC
        enable_lapic();

        let addr = {
            let page_table = PAGE_TABLE.lock();
            let ptable = page_table.as_ref().unwrap();

            let phys_addr = cpu_get_lapic_base();
            ptable.phys_offset() + phys_addr
        };

        let mut lapic = Self { addr };

        lapic.verify()?;

        Ok(lapic)
    }

    fn verify(&mut self) -> Result<(), Error> {
        const LVR_MASK: u32 = 0xff_00ff;
        const APIC_ID_MASK: u32 = 0xFF << 24;

        let ver = self.read(RegisterOffset::Version);
        self.write(RegisterOffset::Version, ver ^ LVR_MASK);

        let ver2 = self.read(RegisterOffset::Version);
        if ver != ver2 {
            return Err(Error::VerificationFailed);
        }

        // Check if versions look reasonable
        let ver2 = ver & 0xff;
        if ver2 == 0 || ver2 == 0xff {
            return Err(Error::VerificationFailed);
        }
        let ver2 = (ver >> 16) & 0xff;
        if ver2 < 0x02 || ver2 == 0xff {
            return Err(Error::VerificationFailed);
        }

        // The ID register is read/write in a real APIC
        let id = self.read(RegisterOffset::ID);
        self.write(RegisterOffset::ID, id ^ APIC_ID_MASK);

        let id2 = self.read(RegisterOffset::ID);
        self.write(RegisterOffset::ID, id);

        if id != (id2 ^ APIC_ID_MASK) {
            return Err(Error::VerificationFailed);
        }

        Ok(())
    }

    fn read(&self, off: RegisterOffset) -> u32 {
        let ptr: *const u32 = (self.addr + u64::from(off.offset())).as_ptr();
        unsafe { ptr.read_volatile() }
    }

    fn write(&mut self, off: RegisterOffset, val: u32) {
        let ptr: *mut u32 = (self.addr + u64::from(off.offset())).as_mut_ptr();
        unsafe { ptr.write_volatile(val) };
    }

    /// Starts the APIC timer to fire an interrupt every 10ms
    pub fn start_timer(&mut self) {
        // Spurious interrupt vector register
        self.write(RegisterOffset::SpuriousInterruptVector, 0x1ff);

        // Task priority register
        self.write(RegisterOffset::TaskPriority, 0);

        // Tell APIC timer to use divider 16
        self.write(RegisterOffset::DivideConfiguration, 0x3);

        // Prepare the PIT to sleep for 10ms (10000µs)
        PIT0.start_timer(OperatingMode::InterruptOnTerminalCount, 100)
            .unwrap();

        // Set APIC init counter to -1
        self.write(RegisterOffset::InitialCount, 0xffff_ffff);

        // Wait for PIT to reach 0
        while PIT0.get_count() != 0 {}

        // Stop APIC timer
        self.write(RegisterOffset::LVTTimer, 0x10000);

        let ticks_per_10ms = 0xFFFF_FFFF - self.read(RegisterOffset::CurrentCount);

        // Start timer as periodic on IRQ 0, divider 16, with the number of ticks we counted
        self.write(RegisterOffset::LVTTimer, 32 | 0x20000);
        self.write(RegisterOffset::DivideConfiguration, 0x3);
        self.write(RegisterOffset::InitialCount, ticks_per_10ms);
    }

    /// Acknowledge an interrupt
    pub fn end_of_interrupt(&mut self) {
        self.write(RegisterOffset::EndOfInterrupt, 0);
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Error {
    NotSupported,
    VerificationFailed,
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
#[repr(u16)]
pub enum RegisterOffset {
    ID = 0x20,
    Version = 0x30,
    TaskPriority = 0x80,
    ArbitrationPriority = 0x90,
    ProcessorPriority = 0xa0,
    EndOfInterrupt = 0xb0,
    RemoteRead = 0xc0,
    LogicalDestination = 0xd0,
    DestinationFormat = 0xe0,
    SpuriousInterruptVector = 0xf0,
    // InService = 0x100,
    // TriggerMode = 0x180,
    // InterruptRequest = 0x200,
    ErrorStatus = 0x280,
    LVTCorrectedMachineCheckInterrupt = 0x2f0,
    InterruptCommandLow = 0x300,
    InterruptCommandHigh = 0x310,
    LVTTimer = 0x320,
    LVTThermalSensor = 0x330,
    LVTPerformanceMonitoringCounters = 0x340,
    LVTLint0 = 0x350,
    LVTLint1 = 0x360,
    LVTError = 0x370,
    InitialCount = 0x380,
    CurrentCount = 0x390,
    DivideConfiguration = 0x3e0,
}

impl RegisterOffset {
    pub const fn offset(self) -> u16 {
        self as u16
    }

    pub const fn can_read(self) -> bool {
        !matches!(self, Self::EndOfInterrupt)
    }

    pub const fn can_write(self) -> bool {
        !matches!(
            self,
            Self::Version
                | Self::ArbitrationPriority
                | Self::ProcessorPriority
                | Self::RemoteRead
                | Self::ErrorStatus
                | Self::CurrentCount
        )
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

fn enable_lapic() {
    cpu_set_lapic_base(cpu_get_lapic_base());
}

fn cpu_get_lapic_base() -> u64 {
    let msr = Msr::new(APIC_BASE_MSR);
    let msr_val = unsafe { msr.read() };

    msr_val & 0x0f_ffff_f000
}

fn cpu_set_lapic_base(base: u64) {
    let mut msr = Msr::new(APIC_BASE_MSR);

    let base = base & 0x0f_ffff_0000 | APIC_BASE_MSR_ENABLE;

    unsafe { msr.write(base) };
}
