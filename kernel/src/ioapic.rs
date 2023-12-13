//! To control an I/O APIC.
//!
//! The IO APIC routes hardware interrupts to a local APIC.
//!
//! Figuring out which (bus,dev,fun,vector) maps to which I/O APIC
//! entry can be a pain.

use bit_field::BitField;
use bitflags::bitflags;
use spin::Mutex;

bitflags! {
    /// The redirection table starts at REG_TABLE and uses
    /// two registers to configure each interrupt.
    /// The first (low) register in a pair contains configuration bits.
    /// The second (high) register contains a bitmask telling which
    /// CPUs can serve that interrupt.
    #[derive(Debug, Copy, Clone)]
    struct RedirectionEntry: u32 {
        /// Interrupt disabled
        const DISABLED  = 0x0001_0000;
        /// Level-triggered (vs edge)
        const LEVEL     = 0x0000_8000;
        /// Active low (vs high)
        const ACTIVELOW = 0x0000_2000;
        /// Destination is CPU id (vs APIC ID)
        const LOGICAL   = 0x0000_0800;
        /// None
        const NONE		= 0x0000_0000;
    }
}

pub static IOAPIC: Mutex<Option<IoApic>> = Mutex::new(None);

pub struct IoApic {
    reg: *mut u32,
    data: *mut u32,
}

unsafe impl Send for IoApic {}

impl IoApic {
    /// Instantiate a new [`IoApic`] from ACPI tables.
    pub fn new() -> Result<Self, Error> {
        let acpi = crate::acpi::get_acpi()?;
        let platform = acpi.platform_info()?;
        let acpi::InterruptModel::Apic(apic) = platform.interrupt_model else {
            return Err(Error::NotSupported);
        };

        let phys_addr = apic.io_apics[0].address;
        let virt_addr = crate::memory::PHYSICAL_MEM_START + u64::from(phys_addr);

        Ok(Self {
            reg: virt_addr.as_mut_ptr(),
            data: (virt_addr + 0x10u64).as_mut_ptr(),
        })
    }

    pub fn disable_all(&mut self) {
        // Mark all interrupts edge-triggered, active high, disabled,
        // and not routed to any CPUs.
        for i in 0..self.supported_interrupts() {
            self.write_irq(i, RedirectionEntry::DISABLED, 0);
        }
    }

    unsafe fn read(&mut self, reg: u8) -> u32 {
        self.reg.write_volatile(u32::from(reg));
        self.data.read_volatile()
    }

    unsafe fn write(&mut self, reg: u8, data: u32) {
        self.reg.write_volatile(u32::from(reg));
        self.data.write_volatile(data);
    }

    fn write_irq(&mut self, irq: u8, flags: RedirectionEntry, dest: u8) {
        unsafe {
            self.write(REG_TABLE + 2 * irq, u32::from(T_IRQ0 + irq) | flags.bits());
            self.write(REG_TABLE + 2 * irq + 1, u32::from(dest) << 24);
        }
    }

    pub fn enable(&mut self, irq: u8, cpunum: u8) {
        // Mark interrupt edge-triggered, active high,
        // enabled, and routed to the given cpunum,
        // which happens to be that cpu's APIC ID.
        self.write_irq(irq, RedirectionEntry::NONE, cpunum);
    }

    pub fn id(&mut self) -> u8 {
        unsafe { self.read(REG_ID).get_bits(24..28) as u8 }
    }

    pub fn version(&mut self) -> u8 {
        unsafe { self.read(REG_VER).get_bits(0..8) as u8 }
    }

    /// Number of supported interrupts by this IO APIC.
    ///
    /// Max Redirection Entry = "how many IRQs can this I/O APIC handle - 1"
    /// The -1 is silly so we add one back to it.
    pub fn supported_interrupts(&mut self) -> u8 {
        unsafe { (self.read(REG_VER).get_bits(16..24) + 1) as u8 }
    }
}

/// Register index: ID
const REG_ID: u8 = 0x00;

/// Register index: version
const REG_VER: u8 = 0x01;

/// Redirection table base
const REG_TABLE: u8 = 0x10;

const T_IRQ0: u8 = 32;

#[derive(Debug)]
pub enum Error {
    NotSupported,
    AcpiError(acpi::AcpiError),
}

impl From<acpi::AcpiError> for Error {
    fn from(err: acpi::AcpiError) -> Self {
        Self::AcpiError(err)
    }
}
