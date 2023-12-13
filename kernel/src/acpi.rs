use core::ptr::NonNull;

use acpi::{AcpiResult, AcpiTables, PhysicalMapping};
use spin::Once;

use crate::memory::PAGE_TABLE;

pub static RDSP_ADDRESS: Once<usize> = Once::new();

pub fn get_acpi() -> AcpiResult<AcpiTables<ACPIHandler>> {
    let rsdp = RDSP_ADDRESS.try_call_once(|| unsafe {
        let mapping = acpi::rsdp::Rsdp::search_for_on_bios(ACPIHandler)?;
        Ok(mapping.physical_start())
    })?;

    unsafe { AcpiTables::from_rsdp(ACPIHandler, *rsdp) }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ACPIHandler;

impl acpi::AcpiHandler for ACPIHandler {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        let phys_offset = PAGE_TABLE.lock().as_mut().unwrap().phys_offset();
        let virt_addr = phys_offset + physical_address;

        PhysicalMapping::new(
            physical_address,
            NonNull::new_unchecked(virt_addr.as_mut_ptr()),
            size,
            size,
            Self,
        )
    }

    fn unmap_physical_region<T>(_: &PhysicalMapping<Self, T>) {
        // Do nothing
    }
}
