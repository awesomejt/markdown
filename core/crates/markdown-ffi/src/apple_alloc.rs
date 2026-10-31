//! Apple allocator wrapper for repeated large documents (MAC-001G).
//!
//! On macOS 27 a `realloc` that grows a block in Apple's large-allocation
//! range leaves the old region resident and dirty ("Malloc Large (empty)"),
//! and later allocations don't reuse it. A parse grows several vectors past
//! that range, so each 5 MiB document kept about 80 MiB of physical footprint
//! after its session closed, without bound. Freeing a block and allocating a
//! new one is reused normally, so large reallocations here move the data
//! themselves. Smaller ones keep `System`'s in-place `realloc`. Every other
//! call forwards to `System` unchanged.

use std::alloc::{GlobalAlloc, Layout, System};

/// Reallocations whose old or new size reaches this many bytes allocate,
/// copy and free instead of calling `realloc`. It is well below the large
/// range, so every affected `realloc` is covered.
pub(crate) const COPY_THRESHOLD: usize = 1 << 20;

pub(crate) struct CopyingRealloc;

#[global_allocator]
static GLOBAL: CopyingRealloc = CopyingRealloc;

// SAFETY: every method forwards to `System`, which upholds the `GlobalAlloc`
// contract; `realloc` only combines `System.alloc` and `System.dealloc`.
unsafe impl GlobalAlloc for CopyingRealloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's layout guarantees are passed through unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as for `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from this allocator, so from `System`, with `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if layout.size().max(new_size) < COPY_THRESHOLD {
            // SAFETY: the caller's `realloc` guarantees are passed through unchanged.
            return unsafe { System.realloc(ptr, layout, new_size) };
        }
        // SAFETY: the caller guarantees `new_size`, rounded up to the
        // alignment, doesn't overflow `isize`, so the layout is valid.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        // SAFETY: `new_size` is nonzero, because the caller guarantees it.
        let new = unsafe { System.alloc(new_layout) };
        if !new.is_null() {
            // SAFETY: both blocks are valid for the smaller size and are
            // distinct allocations; `ptr` is then freed with its own layout.
            unsafe {
                std::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
                System.dealloc(ptr, layout);
            }
        }
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growing_and_shrinking_across_the_threshold_keeps_contents() {
        // Through the global allocator, as the parser's vectors grow.
        let count = COPY_THRESHOLD / 8 * 4;
        let mut values: Vec<u64> = Vec::new();
        for value in 0..count as u64 {
            values.push(value);
        }
        assert!(values.iter().enumerate().all(|(i, &v)| v == i as u64));
        values.truncate(10);
        values.shrink_to_fit();
        assert_eq!(values, (0..10).collect::<Vec<u64>>());
    }

    #[test]
    fn large_reallocation_keeps_alignment_and_contents() {
        let small = Layout::from_size_align(4096, 4096).unwrap();
        let large = Layout::from_size_align(COPY_THRESHOLD * 2, 4096).unwrap();
        // SAFETY: nonzero layouts; each pointer is used within its current
        // size and freed once with the layout it was last given.
        unsafe {
            let first = CopyingRealloc.alloc(small);
            assert!(!first.is_null());
            for i in 0..small.size() {
                *first.add(i) = i as u8;
            }
            let grown = CopyingRealloc.realloc(first, small, large.size());
            assert!(!grown.is_null());
            assert_eq!(grown as usize % large.align(), 0);
            assert!((0..small.size()).all(|i| *grown.add(i) == i as u8));
            let shrunk = CopyingRealloc.realloc(grown, large, 100);
            assert!(!shrunk.is_null());
            assert_eq!(shrunk as usize % large.align(), 0);
            assert!((0..100).all(|i| *shrunk.add(i) == i as u8));
            CopyingRealloc.dealloc(shrunk, Layout::from_size_align(100, 4096).unwrap());
        }
    }
}
