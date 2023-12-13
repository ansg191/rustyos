use core::{
    alloc::{AllocError, Allocator, GlobalAlloc, Layout},
    ptr::NonNull,
};

use spin::lock_api::Mutex;
use static_assertions::assert_eq_size;
use x86_64::{
    structures::paging::{page::PageRange, Page, Size4KiB},
    VirtAddr,
};

use crate::memory::{
    alloc_kpage, free_kpage,
    layout::{ALLOCATOR_END, ALLOCATOR_START},
    FRAME_ALLOCATOR,
};

const ENTRIES_LEN: usize = 170;

/// Memory allocator that allocates full pages.
///
/// Returns kernel-only memory with flags `PRESENT | WRITABLE`.
pub struct FullPageAllocator {
    inner: Mutex<Option<NonNull<FPAInner>>>,
}

type FPAGuard<'a> = lock_api::MappedMutexGuard<'a, spin::Mutex<()>, FPAInner>;

struct FPAInner {
    entries: [Entry; ENTRIES_LEN],
    prev: Option<NonNull<FPAInner>>,
    next: Option<NonNull<FPAInner>>,
}

#[derive(Debug, Clone, Copy)]
enum Entry {
    Empty,
    Usable { start: VirtAddr, pages: u64 },
}

assert_eq_size!(Entry, [u8; 24]);
assert_eq_size!(FPAInner, [u8; 0x1000]);

unsafe impl Send for FullPageAllocator {}
unsafe impl Sync for FullPageAllocator {}

impl FullPageAllocator {
    /// Create a new full page allocator.
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Retrieve the inner FPAInner struct or initialize it if it doesn't exist.
    fn init_or_get(&self) -> Result<FPAGuard, AllocError> {
        let mut inner = self.inner.lock();
        if inner.is_none() {
            add_entry_page(&mut inner)?;
        }

        Ok(
            lock_api::MutexGuard::try_map(inner, |x| x.as_mut().map(|p| unsafe { p.as_mut() }))
                .map_err(|_| AllocError)?,
        )
    }
}

unsafe impl Allocator for FullPageAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let size = layout.size();
        let num_pages = size.div_ceil(4096);

        let mut inner = self.init_or_get()?;

        let addr = inner.find_free_pages(num_pages as u64).ok_or(AllocError)?;
        inner.alloc_pages(addr, num_pages as u64);

        // Allocate pages
        let mut fr_alloc = FRAME_ALLOCATOR.lock();
        let alloc = fr_alloc.as_mut().unwrap();

        for i in 0..num_pages {
            let page = addr + i * 0x1000;
            unsafe { alloc_kpage(alloc, page) }?;
        }

        Ok(NonNull::slice_from_raw_parts(
            NonNull::new(addr.as_mut_ptr()).ok_or(AllocError)?,
            size,
        ))
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let size = layout.size();
        let num_pages = size.div_ceil(4096);

        let pages = PageRange::<Size4KiB> {
            start: Page::containing_address(VirtAddr::from_ptr(ptr.as_ptr())),
            end: Page::containing_address(VirtAddr::from_ptr(ptr.as_ptr()) + 0x1000 * num_pages),
        };

        let mut fr_alloc = FRAME_ALLOCATOR.lock();
        let alloc = fr_alloc.as_mut().unwrap();

        for page in pages {
            unsafe { free_kpage(alloc, page.start_address()) };
        }
    }

    // unsafe fn grow(
    //     &self,
    //     ptr: NonNull<u8>,
    //     old_layout: Layout,
    //     new_layout: Layout,
    // ) -> Result<NonNull<[u8]>, AllocError> {
    //     todo!()
    // }
    //
    // unsafe fn shrink(
    //     &self,
    //     ptr: NonNull<u8>,
    //     old_layout: Layout,
    //     new_layout: Layout,
    // ) -> Result<NonNull<[u8]>, AllocError> {
    //     todo!()
    // }
}

unsafe impl GlobalAlloc for FullPageAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocate(layout)
            .map(|x| x.as_mut_ptr())
            .unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocate(NonNull::new_unchecked(ptr), layout)
    }
}

impl FPAInner {
    fn find_free_pages(&self, req_pages: u64) -> Option<VirtAddr> {
        for entry in self.entries.iter() {
            let Entry::Usable { start, pages } = *entry else {
                return None;
            };
            if pages >= req_pages {
                return Some(start);
            }
        }

        // No free pages found in this entry page, search next
        if let Some(next) = self.next {
            unsafe { next.as_ref().find_free_pages(req_pages) }
        } else {
            None
        }
    }

    fn insert_entry(&mut self, idx: usize, entry: Entry) -> Result<(), AllocError> {
        // If last entry is usable, move it to next page
        if let Entry::Usable { .. } = self.entries[ENTRIES_LEN - 1] {
            let entry = self.entries[ENTRIES_LEN - 1];
            if let Some(mut next) = self.next {
                unsafe { next.as_mut().insert_entry(0, entry) }?;
            } else {
                // Allocate new entry page
                let page = self.add_entry_page()?;
                page.insert_entry(0, entry)?;
            }
        }

        // Shift entries up
        self.entries.copy_within(idx..ENTRIES_LEN - 1, idx + 1);

        // Insert new entry
        self.entries[idx] = entry;

        Ok(())
    }

    fn append_entry(&mut self, entry: Entry) -> Result<(), AllocError> {
        // Find first empty entry
        let idx = self.entries.iter().position(|e| matches!(e, Entry::Empty));

        if let Some(idx) = idx {
            self.entries[idx] = entry;
        } else {
            // No empty entries found
            // If next page exists, append to that
            // Else, add new entry page
            if let Some(mut next) = self.next {
                unsafe { next.as_mut().append_entry(entry) }?;
            } else {
                // Allocate new entry page
                let page = self.add_entry_page()?;
                page.append_entry(entry)?;
            }
        }

        Ok(())
    }

    fn remove_entry(&mut self, idx: usize) -> Entry {
        let end = if let Some(mut next) = self.next {
            Some(unsafe { next.as_mut().remove_entry(0) })
        } else {
            None
        };

        let entry = core::mem::replace(&mut self.entries[idx], Entry::Empty);

        // Shift entries down
        self.entries.copy_within(idx + 1..ENTRIES_LEN, idx);

        // Set last entry to end
        match end {
            None => {}
            Some(Entry::Empty) => {
                // Remove last entry page
                self.remove_entry_page();
            }
            Some(end) => {
                self.entries[ENTRIES_LEN - 1] = end;
            }
        }

        entry
    }

    /// Squashes adjacent entries together.
    fn squash_entries(&mut self) {
        let mut i = 0;
        while i < ENTRIES_LEN - 1 {
            let Entry::Usable { start, pages } = self.entries[i] else {
                return;
            };

            let Entry::Usable {
                start: next_start,
                pages: next_pages,
            } = self.entries[i + 1]
            else {
                return;
            };

            if start + pages * 0x1000 == next_start {
                // Squash entries together
                self.entries[i] = Entry::Usable {
                    start,
                    pages: pages + next_pages,
                };

                // Shift entries down
                self.remove_entry(i + 1);

                // Don't increment i
            } else {
                i += 1;
            }
        }

        // If next page exists, squash entries together
        if let Some(mut next) = self.next {
            unsafe { next.as_mut().squash_entries() };
        }
    }

    fn alloc_pages(&mut self, start: VirtAddr, pages: u64) {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            let Entry::Usable { start: s, pages: p } = *entry else {
                return;
            };

            if s != start {
                continue;
            }

            if p == pages {
                // Exact match, remove entry
                self.remove_entry(i);
            } else {
                // Partial match, shrink entry
                *entry = Entry::Usable {
                    start: start + pages * 0x1000,
                    pages: p - pages,
                };
            }

            return;
        }

        // No match found in this entry page, search next
        if let Some(mut next) = self.next {
            unsafe { next.as_mut().alloc_pages(start, pages) }
        } else {
            panic!("No match found in any entry page");
        }
    }

    /// Return pages back to the allocator.
    ///
    /// # Safety
    ///
    /// `start` must be page aligned & have been allocated by this allocator.
    unsafe fn dealloc_pages(&mut self, start: VirtAddr, pages: u64) -> Result<(), AllocError> {
        // Find entry with address greater than start
        for (i, entry) in self.entries.iter_mut().enumerate() {
            let Entry::Usable { start: s, pages: p } = *entry else {
                return Ok(());
            };

            if s < start {
                continue;
            }

            // Check if we can add directly to this entry
            if s == start + pages * 0x1000 {
                // Add to start of entry
                *entry = Entry::Usable {
                    start,
                    pages: p + pages,
                };
                return Ok(());
            }

            // Insert new entry before this one
            let new_entry = Entry::Usable { start, pages };
            self.insert_entry(i, new_entry)?;
            self.squash_entries();
            return Ok(());
        }

        // Check next entry page
        if let Some(mut next) = self.next {
            unsafe { next.as_mut().dealloc_pages(start, pages) }
        } else {
            // No match found, add new entry to end
            let new_entry = Entry::Usable { start, pages };
            self.append_entry(new_entry)?;
            self.squash_entries();
            Ok(())
        }
    }

    /// Append a new entry page to the linked list.
    fn add_entry_page(&mut self) -> Result<&mut Self, AllocError> {
        let mut inner = Some(NonNull::from(&*self));
        add_entry_page(&mut inner)?;
        Ok(unsafe { self.next.unwrap().as_mut() })
    }

    /// Removes last entry page from linked list.
    fn remove_entry_page(&mut self) {
        let mut inner = Some(NonNull::from(&*self));
        remove_entry_page(&mut inner);
    }
}

fn add_entry_page(inner: &mut Option<NonNull<FPAInner>>) -> Result<(), AllocError> {
    match inner {
        None => {
            {
                // Allocate page at ALLOCATOR_START
                let mut alloc = FRAME_ALLOCATOR.lock();
                unsafe { alloc_kpage(alloc.as_mut().unwrap(), ALLOCATOR_START) }?;
            }

            // Init page at ALLOCATOR_START as FPAInner
            let ptr = ALLOCATOR_START.as_mut_ptr::<FPAInner>();
            let first = unsafe {
                *ptr = FPAInner {
                    entries: [Entry::Empty; ENTRIES_LEN],
                    prev: None,
                    next: None,
                };

                &mut (*ptr).entries[0]
            };

            // Set first entry to free
            *first = Entry::Usable {
                start: ALLOCATOR_START + 0x1000u64,
                pages: (ALLOCATOR_END.align_up(0x1000u64) - ALLOCATOR_START + 0x1000) / 0x1000,
            };

            *inner = Some(unsafe { NonNull::new_unchecked(ptr) });
        }
        Some(in_ptr) => {
            let inner = unsafe { in_ptr.as_mut() };

            // Find a free page
            let free_page = inner.find_free_pages(1).ok_or(AllocError)?;
            inner.alloc_pages(free_page, 1);

            // Allocate page
            {
                let mut alloc = FRAME_ALLOCATOR.lock();
                unsafe { alloc_kpage(alloc.as_mut().unwrap(), free_page) }?;
            }

            // Init page as FPAInner
            let ptr = free_page.as_mut_ptr::<FPAInner>();
            unsafe {
                *ptr = FPAInner {
                    entries: [Entry::Empty; ENTRIES_LEN],
                    prev: Some(*in_ptr),
                    next: None,
                };
            };
            inner.next = Some(unsafe { NonNull::new_unchecked(ptr) });
        }
    }
    Ok(())
}

fn remove_entry_page(fpa_inner: &mut Option<NonNull<FPAInner>>) {
    match fpa_inner {
        None => panic!("No entry pages to remove"),
        Some(inner) => {
            let inner = unsafe { inner.as_mut() };

            // Find last entry page
            let mut last = inner;
            while let Some(mut next) = last.next {
                last = unsafe { next.as_mut() };
            }

            let prev = last.prev;

            // Remove entry page
            {
                let mut alloc = FRAME_ALLOCATOR.lock();
                unsafe {
                    free_kpage(
                        alloc.as_mut().unwrap(),
                        VirtAddr::from_ptr(last as *const _),
                    )
                };
            }

            // Remove entry page from linked list
            if let Some(mut prev) = prev {
                unsafe { prev.as_mut().next = None };
            } else {
                *fpa_inner = None;
            }
        }
    }
}
