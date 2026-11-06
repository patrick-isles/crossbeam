use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crossbeam_queue::SegQueue;

struct CountingAlloc;

static BLOCK_ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static BLOCK_DEALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static TEST_MUTEX: Mutex<()> = Mutex::new(());

// Block<usize> layout size is exactly 504 bytes on 64-bit systems.
const BLOCK_SIZE: usize = 504;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == BLOCK_SIZE {
            BLOCK_ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() == BLOCK_SIZE {
            BLOCK_DEALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        System.dealloc(ptr, layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if layout.size() == BLOCK_SIZE {
            BLOCK_ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if layout.size() == BLOCK_SIZE {
            BLOCK_DEALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        if new_size == BLOCK_SIZE {
            BLOCK_ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static A: CountingAlloc = CountingAlloc;

#[test]
fn test_segqueue_push_pop_zero_allocations() {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let q: SegQueue<usize> = SegQueue::new();

    // Warm up one lap (32 elements) so the initial block and first transition
    // retire a block into the cache.
    for i in 0..32 {
        q.push(i);
        assert_eq!(q.pop(), Some(i));
    }

    let allocs_before = BLOCK_ALLOC_COUNT.load(Ordering::SeqCst);
    let deallocs_before = BLOCK_DEALLOC_COUNT.load(Ordering::SeqCst);

    // Perform 10,000 push and pop cycles across hundreds of laps.
    for i in 0..10_000 {
        q.push(i);
        assert_eq!(q.pop(), Some(i));
    }

    let allocs_during = BLOCK_ALLOC_COUNT.load(Ordering::SeqCst) - allocs_before;
    let deallocs_during = BLOCK_DEALLOC_COUNT.load(Ordering::SeqCst) - deallocs_before;

    println!(
        "Block allocations across 10,000 push/pop cycles: allocs={}, deallocs={}",
        allocs_during, deallocs_during
    );

    // In an optimal zero-allocation recycled queue, no additional blocks are allocated or deallocated.
    assert_eq!(
        allocs_during, 0,
        "SegQueue allocated {} blocks during 10,000 push/pop cycles",
        allocs_during
    );
    assert_eq!(
        deallocs_during, 0,
        "SegQueue deallocated {} blocks during 10,000 push/pop cycles",
        deallocs_during
    );
}

#[test]
fn test_segqueue_push_mut_pop_mut_zero_allocations() {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let mut q: SegQueue<usize> = SegQueue::new();

    // Warm up one lap (32 elements) with push_mut/pop_mut
    for i in 0..32 {
        q.push_mut(i);
        assert_eq!(q.pop_mut(), Some(i));
    }

    let allocs_before = BLOCK_ALLOC_COUNT.load(Ordering::SeqCst);
    let deallocs_before = BLOCK_DEALLOC_COUNT.load(Ordering::SeqCst);

    for i in 0..10_000 {
        q.push_mut(i);
        assert_eq!(q.pop_mut(), Some(i));
    }

    let allocs_during = BLOCK_ALLOC_COUNT.load(Ordering::SeqCst) - allocs_before;
    let deallocs_during = BLOCK_DEALLOC_COUNT.load(Ordering::SeqCst) - deallocs_before;

    assert_eq!(
        allocs_during, 0,
        "SegQueue allocated {} blocks during push_mut/pop_mut",
        allocs_during
    );
    assert_eq!(
        deallocs_during, 0,
        "SegQueue deallocated {} blocks during push_mut/pop_mut",
        deallocs_during
    );
}
