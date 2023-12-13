pub mod allocator;
pub mod frame;
pub mod layout;

use core::alloc::AllocError;

use bootloader_api::info::MemoryRegions;
use spin::Mutex;
use x86_64::{
    structures::paging::{
        mapper::CleanUp, page::PageRangeInclusive, FrameAllocator, FrameDeallocator, Mapper,
        OffsetPageTable, Page, PageTable, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

pub use self::layout::PHYSICAL_MEM_START;
use crate::memory::frame::BitmapFrameAllocator;

pub static PAGE_TABLE: Mutex<Option<OffsetPageTable<'static>>> = Mutex::new(None);
pub static FRAME_ALLOCATOR: Mutex<Option<BitmapFrameAllocator>> = Mutex::new(None);

#[global_allocator]
pub static ALLOCATOR: allocator::KAllocator = allocator::KAllocator::new();

pub static PAGE_ALLOCATOR: allocator::FullPageAllocator = allocator::FullPageAllocator::new();

fn active_level_4_table() -> &'static mut PageTable {
    let (level_4_table, _) = x86_64::registers::control::Cr3::read();

    let phys = level_4_table.start_address();
    let virt = PHYSICAL_MEM_START + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    // SAFETY: We know that the physical address space is mapped to the virtual address space
    // at PHYSICAL_MEM_START
    unsafe { &mut *page_table_ptr }
}

pub fn init() {
    let level_4_table = active_level_4_table();
    // SAFETY: We know that the physical address space is mapped to the virtual address space
    // at PHYSICAL_MEM_START
    *PAGE_TABLE.lock() = Some(unsafe { OffsetPageTable::new(level_4_table, PHYSICAL_MEM_START) })
}

pub fn init_frame_allocator(memory_regions: &'static MemoryRegions) {
    init();
    let mut ptable = PAGE_TABLE.lock();
    let pt = ptable.as_mut().unwrap();
    let frame_alloc = BitmapFrameAllocator::new(memory_regions, pt);
    *FRAME_ALLOCATOR.lock() = Some(frame_alloc);
}

/// Allocate a single kernel page.
///
/// Very simple, to be used for allocators only.
/// Allocates a single frame from `memory` and maps it to `virt_addr`.
///
/// # Safety
///
/// Page table & unmanaged memory allocations are inherently unsafe.
unsafe fn alloc_kpage(
    alloc: &mut impl FrameAllocator<Size4KiB>,
    virt_addr: VirtAddr,
) -> Result<(), AllocError> {
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    let frame = alloc.allocate_frame().ok_or(AllocError)?;
    let page: Page<Size4KiB> = Page::containing_address(virt_addr);

    if cfg!(debug_assertions) {
        crate::kprintln!("DEBUG: Allocating {:?} for {:?}", frame, page);
    }

    PAGE_TABLE
        .lock()
        .as_mut()
        .ok_or(AllocError)?
        .map_to_with_table_flags(page, frame, flags, flags, alloc)
        .unwrap()
        .flush();

    Ok(())
}

unsafe fn free_kpage(alloc: &mut impl FrameDeallocator<Size4KiB>, virt_addr: VirtAddr) {
    let page: Page<Size4KiB> = Page::containing_address(virt_addr);

    let mut page_table = PAGE_TABLE.lock();
    let pt = page_table.as_mut().unwrap();

    if cfg!(debug_assertions) {
        crate::kprintln!("DEBUG: Freeing {:?}", page);
    }

    // Fill page with zeros to catch dangling pointers
    if cfg!(debug_assertions) {
        let slice = core::slice::from_raw_parts_mut(page.start_address().as_mut_ptr::<u8>(), 4096);
        slice.fill(0);
    }

    let (frame, flush) = pt.unmap(page).unwrap();
    flush.flush();
    alloc.deallocate_frame(frame);

    pt.clean_up_addr_range(
        PageRangeInclusive {
            start: page,
            end: page,
        },
        alloc,
    );
}
