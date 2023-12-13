#![feature(allocator_api, abi_x86_interrupt, asm_const, never_type)]
#![feature(slice_ptr_get)]
#![warn(clippy::pedantic, clippy::nursery)]
#![allow(
    dead_code,
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation
)]
#![no_std]
#![no_main]

extern crate alloc;

mod acpi;
mod lapic;
mod memory;
mod mp;
mod panic;
mod pit;
mod serial;
mod trap;

const BOOT_CONFIG: bootloader_api::BootloaderConfig = {
    let mut config = bootloader_api::BootloaderConfig::new_default();
    config.kernel_stack_size = 64 * 1024;
    config.mappings = bootloader_api::config::Mappings::new_default();
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::FixedAddress(
        memory::PHYSICAL_MEM_START.as_u64(),
    ));
    config
};
bootloader_api::entry_point!(kmain, config = &BOOT_CONFIG);

/// The entry point for the kernel.
///
/// # Panics
///
/// Panics if the kernel crashes.
pub fn kmain(info: &'static mut bootloader_api::BootInfo) -> ! {
    trap::init_idt();
    memory::init();
    memory::init_frame_allocator(&info.memory_regions);

    let mut lpic = lapic::Lapic::new().unwrap();
    lpic.start_timer();
    *lapic::LAPIC.lock() = Some(lpic);
    x86_64::instructions::interrupts::enable();

    kprintln!("Hello, world!");
    kprintln!(
        "Physical memory offset: {:x}",
        info.physical_memory_offset.into_option().unwrap()
    );

    let regions = &*info.memory_regions;

    kprintln!("Memory regions:");
    for region in regions {
        kprintln!(
            "\t{:x} - {:x} ({:?})",
            region.start,
            region.end,
            region.kind
        );
    }

    kprintln!("kmain address: {:x}", kmain as usize);

    kprintln!(
        "ALLOCATOR MEM RANGE: {:x} - {:x}",
        memory::layout::ALLOCATOR_START.as_u64(),
        memory::layout::ALLOCATOR_END.as_u64()
    );
    {
        let acpi = acpi::get_acpi().unwrap();
        let platform = acpi.platform_info().unwrap();
        kprintln!("Platform: {:#?}", platform);
    }

    // {
    //     let mut arr = alloc::vec::Vec::<Box<_>>::with_capacity(512);
    //     for i in 0..arr.capacity() {
    //         let boxed = Box::new(i as u128);
    //         // kprintln!("Addr: {:x}", boxed.as_ref() as *const u128 as usize);
    //         arr.push(boxed);
    //     }
    //     kprintln!("{:?}", arr);
    //
    // }
    // {
    //     let x = Box::new(43u128);
    //     kprintln!("{:?}", x);
    // }

    // let mut frame_alloc = memory::frame::BitmapFrameAllocator::new(&info.memory_regions);
    // let frame = frame_alloc.allocate_frame();
    // kprintln!("{frame:?}");
    // unsafe {frame_alloc.deallocate_frame(frame.unwrap())};

    kprintln!("No Crash!");
    loop {
        x86_64::instructions::interrupts::enable_and_hlt();
    }
}
