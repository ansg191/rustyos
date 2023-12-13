pub mod boot;

use core::mem::size_of;

use bootloader_api::info::{MemoryRegion, MemoryRegionKind, MemoryRegions};
use x86_64::{
    structures::paging::{
        FrameAllocator, FrameDeallocator, Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame,
        Size4KiB,
    },
    PhysAddr,
};

use crate::memory::{frame::boot::BootFrameAllocator, layout::BITMAP_FRAME_ALLOCATOR_START};

/// Bitmap frame allocator.
///
/// This allocator uses a bitmap to keep track of which frames are free.
/// The bitmap is stored in the first N frames, where N is the number of frames required to store the bitmap for the
/// entire physical memory space.
pub struct BitmapFrameAllocator {
    regions: &'static MemoryRegions,
    bitmap: &'static mut [u64],
}

unsafe impl Send for BitmapFrameAllocator {}
// unsafe impl Sync for BitmapFrameAllocator {}

impl BitmapFrameAllocator {
    /// Creates a new frame allocator with the given memory regions.
    /// Automatically creates a [`BootFrameAllocator`] to bootstrap the allocator.
    pub fn new(regions: &'static MemoryRegions, pt: &mut OffsetPageTable<'static>) -> Self {
        let alloc = BootFrameAllocator::new(regions);
        Self::new_with_alloc(regions, pt, alloc)
    }

    /// Creates a new frame allocator with the given memory regions and a boot frame allocator.
    pub fn new_with_alloc(
        regions: &'static MemoryRegions,
        pt: &mut OffsetPageTable<'static>,
        alloc: BootFrameAllocator,
    ) -> Self {
        let bitmap = Self::allocate_bitmap(regions, pt, alloc);
        Self { regions, bitmap }
    }

    /// Calculate the required size of the bitmap in bytes.
    fn required_bitmap_size(regions: &MemoryRegions) -> u64 {
        let total_bytes =
            usable_regions(regions).fold(0, |acc, region| acc + region.end - region.start);

        total_bytes.div_ceil(4096).div_ceil(8)
    }

    /// Allocate required space for the bitmap in the first usable frame.
    fn allocate_bitmap(
        regions: &MemoryRegions,
        pt: &mut OffsetPageTable<'static>,
        mut alloc: BootFrameAllocator,
    ) -> &'static mut [u64] {
        let bitmap_size = Self::required_bitmap_size(regions);
        let bitmap_frames = bitmap_size.div_ceil(4096);

        let first_frame = alloc.allocate_frame().unwrap();
        let mut last_frame = first_frame;
        for _ in 1..bitmap_frames {
            last_frame = alloc.allocate_frame().unwrap();
        }

        // Check contiguity
        assert!(
            is_contiguous(first_frame, last_frame, bitmap_frames),
            "Bitmap frames are not contiguous"
        );

        for frame in 0..bitmap_frames {
            let phys_addr = first_frame.start_address() + frame * 4096;
            let virt_addr = BITMAP_FRAME_ALLOCATOR_START + frame * 4096;
            let page: Page<Size4KiB> = Page::containing_address(virt_addr);
            let frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(phys_addr);
            unsafe {
                pt.map_to_with_table_flags(
                    page,
                    frame,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    &mut alloc,
                )
                .unwrap()
                .flush();
            }
        }

        let slice = unsafe {
            core::slice::from_raw_parts_mut(
                BITMAP_FRAME_ALLOCATOR_START.as_mut_ptr(),
                bitmap_frames as usize * (4096 / size_of::<u64>()),
            )
        };

        // Zero out the bitmap
        slice.fill(0);

        // Mark the bitmap frames as used
        for i in 0..alloc.used() {
            Self::mark_frame_used(slice, i as u64);
        }

        slice
    }

    #[inline]
    fn mark_frame_used(bitmap: &mut [u64], frame: u64) {
        let word = frame / 64;
        let bit = 63 - (frame % 64);
        bitmap[word as usize] |= 1 << bit;
    }

    #[inline]
    fn mark_frame_free(bitmap: &mut [u64], frame: u64) {
        let word = frame / 64;
        let bit = 63 - (frame % 64);
        bitmap[word as usize] &= !(1 << bit);
    }

    /// Find the first free frame in the bitmap.
    fn first_free_frame(&self) -> Option<u64> {
        for (i, word) in self.bitmap.iter().enumerate() {
            if *word != 0 {
                // Found a word with an empty frame
                let bit: u64 = u64::from(word.leading_ones());
                return Some(i as u64 * 64 + bit);
            }
        }
        None
    }

    /// Convert a frame number to a physical address.
    fn frame_to_address(&self, mut frame: u64) -> Option<PhysAddr> {
        for region in usable_regions(self.regions) {
            let frames = (region.end - region.start) / 4096;
            if frame < frames {
                return Some(PhysAddr::new(region.start + frame * 4096));
            }
            frame -= frames;
        }
        None
    }

    /// Convert a physical address to a frame number.
    fn address_to_frame(&self, addr: PhysAddr) -> Option<u64> {
        let mut frame = 0;
        for region in usable_regions(self.regions) {
            if addr.as_u64() >= region.start && addr.as_u64() < region.end {
                return Some(frame + (addr.as_u64() - region.start) / 4096);
            }
            frame += (region.start - region.end) / 4096;
        }
        None
    }
}

unsafe impl FrameAllocator<Size4KiB> for BitmapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        // Find first free frame
        let frame = self.first_free_frame()?;

        // Mark frame as used
        Self::mark_frame_used(self.bitmap, frame);

        // Calculate frame start address
        let addr = self.frame_to_address(frame)?;

        Some(PhysFrame::from_start_address(addr).expect("All frame address are page aligned"))
    }
}

impl FrameDeallocator<Size4KiB> for BitmapFrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        let frame = self
            .address_to_frame(frame.start_address())
            .expect("frame should be located in regions");

        Self::mark_frame_free(self.bitmap, frame);
    }
}

fn usable_regions(regions: &MemoryRegions) -> impl Iterator<Item = &MemoryRegion> {
    regions
        .iter()
        .filter(|region| region.kind == MemoryRegionKind::Usable)
}

/// Check if a range of frames is contiguous.
const fn is_contiguous(start: PhysFrame, end: PhysFrame, pages: u64) -> bool {
    let start = start.start_address().as_u64();
    let end = end.start_address().as_u64() + 0x1000;
    let pages = pages * 0x1000;

    end - start == pages
}
