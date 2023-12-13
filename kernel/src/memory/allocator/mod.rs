pub mod page;

use alloc::boxed::Box;
use core::{
    alloc::{AllocError, Allocator, GlobalAlloc, Layout},
    ptr::NonNull,
};

use spin::Mutex;
use x86_64::{
    structures::paging::{Page, PageSize, Size4KiB},
    VirtAddr,
};

pub use self::page::FullPageAllocator;
use crate::memory::PAGE_ALLOCATOR;

/// Default kernel allocator.
///
/// This is the global allocator used by the kernel.
/// It has buckets for various sizes, and falls back to the page allocator for larger allocations.
///
/// Returns kernel-only memory with flags `PRESENT | WRITABLE`.
#[derive(Debug)]
pub struct KAllocator {
    buckets: Mutex<Buckets>,
}

#[derive(Debug)]
struct Buckets(
    Option<Bucket<64, 8>>,   // 8 bytes
    Option<Bucket<32, 16>>,  // 16 bytes
    Option<Bucket<16, 32>>,  // 32 bytes
    Option<Bucket<8, 64>>,   // 64 bytes
    Option<Bucket<4, 128>>,  // 128 bytes
    Option<Bucket<2, 256>>,  // 256 bytes
    Option<Bucket<1, 512>>,  // 512 bytes
    Option<Bucket<1, 1024>>, // 1024 bytes
    Option<Bucket<1, 2048>>, // 2048 bytes
);

#[derive(Debug)]
struct Bucket<const SIZE: usize, const BLOCK: u64> {
    page: Page,
    bitmap: [u8; SIZE],
    // TODO: Change this to not use FullPageAllocator
    next: Option<Box<Bucket<SIZE, BLOCK>, &'static FullPageAllocator>>,
}

impl<const SIZE: usize, const BLOCK: u64> Drop for Bucket<SIZE, BLOCK> {
    fn drop(&mut self) {
        let ptr = unsafe { NonNull::new_unchecked(self.page.start_address().as_mut_ptr()) };
        unsafe {
            PAGE_ALLOCATOR.deallocate(ptr, Layout::new::<u8>());
        }
    }
}

impl KAllocator {
    pub const fn new() -> Self {
        Self {
            buckets: Mutex::new(Buckets(
                None, None, None, None, None, None, None, None, None,
            )),
        }
    }
}

macro_rules! allocate {
    ($buckets:ident, $idx:tt) => {
        if let Some(bucket) = &mut $buckets.$idx {
            bucket.allocate_block()?
        } else {
            let mut bucket = Bucket::new()?;
            let addr = bucket.allocate_block()?;
            $buckets.$idx = Some(bucket);
            addr
        }
    };
}

macro_rules! deallocate {
    ($buckets:ident, $idx:tt, $addr:ident) => {
        if let Some(bucket) = &mut $buckets.$idx {
            bucket.free_block($addr)
        } else {
            panic!("invalid free")
        }
    };
}

unsafe impl Allocator for KAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let size = layout.size();
        let align = layout.align();
        let max = size.max(align);

        let mut buckets = self.buckets.lock();

        let addr = match max {
            0..=8 => allocate!(buckets, 0),
            9..=16 => allocate!(buckets, 1),
            17..=32 => allocate!(buckets, 2),
            33..=64 => allocate!(buckets, 3),
            65..=128 => allocate!(buckets, 4),
            129..=256 => allocate!(buckets, 5),
            257..=512 => allocate!(buckets, 6),
            513..=1024 => allocate!(buckets, 7),
            1025..=2048 => allocate!(buckets, 8),
            _ => {
                // Fall back to page allocator
                if align as u64 > Size4KiB::SIZE {
                    panic!("invalid alignment");
                }
                return PAGE_ALLOCATOR.allocate(layout);
            }
        };

        Ok(NonNull::slice_from_raw_parts(
            NonNull::new(addr.as_mut_ptr()).ok_or(AllocError)?,
            size,
        ))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let size = layout.size();
        let align = layout.align();
        let max = size.max(align);

        let addr = VirtAddr::from_ptr(ptr.as_ptr());

        let mut buckets = self.buckets.lock();

        match max {
            0..=8 => deallocate!(buckets, 0, addr),
            9..=16 => deallocate!(buckets, 1, addr),
            17..=32 => deallocate!(buckets, 2, addr),
            33..=64 => deallocate!(buckets, 3, addr),
            65..=128 => deallocate!(buckets, 4, addr),
            129..=256 => deallocate!(buckets, 5, addr),
            257..=512 => deallocate!(buckets, 6, addr),
            513..=1024 => deallocate!(buckets, 7, addr),
            1025..=2048 => deallocate!(buckets, 8, addr),
            _ => {
                // Fall back to page allocator
                if align as u64 > Size4KiB::SIZE {
                    panic!("invalid alignment");
                }
                PAGE_ALLOCATOR.deallocate(ptr, layout);
            }
        }
    }
}

unsafe impl GlobalAlloc for KAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocate(layout)
            .map(|x| x.as_mut_ptr())
            .unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocate(NonNull::new_unchecked(ptr), layout)
    }
}

impl<const SIZE: usize, const BLOCK: u64> Bucket<SIZE, BLOCK> {
    fn new() -> Result<Self, AllocError> {
        let layout = Layout::new::<u8>();
        let page_ptr = PAGE_ALLOCATOR.allocate(layout)?.as_mut_ptr();
        let page = Page::containing_address(VirtAddr::from_ptr(page_ptr));

        Ok(Self {
            page,
            bitmap: [0; SIZE],
            next: None,
        })
    }

    fn is_empty(&self) -> bool {
        self.bitmap.iter().all(|byte| *byte == 0)
    }

    fn allocate_block(&mut self) -> Result<VirtAddr, AllocError> {
        let mut offset = None;
        for (i, byte) in self.bitmap.iter_mut().enumerate() {
            if *byte == 0xFF {
                continue;
            }

            let leading = byte.leading_ones();
            let bit = 7 - leading;

            let off = i as u64 * 8 + leading as u64;
            if off >= Size4KiB::SIZE / BLOCK {
                continue;
            }

            offset = Some(off);
            *byte |= 1 << bit;
            break;
        }

        if let Some(offset) = offset {
            return Ok(self.page.start_address() + offset * BLOCK);
        }

        if let Some(next) = &mut self.next {
            next.allocate_block()
        } else {
            let mut next = Box::new_in(Self::new()?, &PAGE_ALLOCATOR);
            let addr = next.allocate_block()?;
            self.next = Some(next);
            Ok(addr)
        }
    }

    fn free_block(&mut self, addr: VirtAddr) {
        if addr.align_down(Size4KiB::SIZE) == self.page.start_address() {
            let offset = (addr - self.page.start_address()) / BLOCK;
            let byte = offset as usize / 8;
            let bit = 7 - (offset as usize % 8);

            self.bitmap[byte] &= !(1 << bit);
        } else if let Some(next) = &mut self.next {
            next.free_block(addr);

            if next.is_empty() {
                self.next = next.next.take();
            }
        }
    }
}
