use bootloader_api::info::MemoryRegions;
use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
    PhysAddr,
};

use crate::memory::frame::usable_regions;

/// Boot-time physical frame allocator.
///
/// Used to bootstrap the kernel's default frame allocator [`BitmapFrameAllocator`].
///
/// It's a very simple allocator that has a counter and returns the next frame from the memory sections.
/// The allocator cannot deallocate frames. It is not meant to be used after [`BitmapFrameAllocator`] is initialized.
///
/// [`BitmapFrameAllocator`]: crate::memory::frame::BitmapFrameAllocator
pub struct BootFrameAllocator {
    regions: &'static MemoryRegions,
    next: usize,
}

impl BootFrameAllocator {
    /// Creates a new frame allocator with the given memory regions & no allocated frames.
    pub const fn new(regions: &'static MemoryRegions) -> Self {
        Self { regions, next: 0 }
    }

    /// Returns the number of frames that have been allocated.
    pub const fn used(&self) -> usize {
        self.next
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        usable_regions(self.regions)
            .map(|r| r.start..r.end)
            .flat_map(|r| r.step_by(4096))
            .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let frame = self.usable_frames().nth(self.next)?;
        self.next += 1;
        Some(frame)
    }
}
