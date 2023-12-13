use bootloader_api::info::MemoryRegions;
use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
    PhysAddr,
};

use crate::memory::frame::usable_regions;

pub struct BootFrameAllocator {
    regions: &'static MemoryRegions,
    next: usize,
}

impl BootFrameAllocator {
    pub fn new(regions: &'static MemoryRegions) -> Self {
        Self { regions, next: 0 }
    }

    pub fn used(&self) -> usize {
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
