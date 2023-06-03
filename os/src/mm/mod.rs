mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::VPNRange;
pub use address::{PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum, align_up};
pub use frame_allocator::{frame_alloc, frame_alloc_more, frame_dealloc, FrameTracker};
pub use memory_set::remap_test;
pub use memory_set::{kernel_token, MapArea, MapPermission, MapType, MemorySet, KERNEL_SPACE, MemoryMapArea};
use page_table::PTEFlags;
pub use page_table::{
    translated_byte_buffer, translated_ref, translated_refmut, translated_str, PageTable,
    PageTableEntry, UserBuffer, UserBufferIterator,
};

pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}

use crate::task::current_process;


pub fn lazy_check(addr: usize) -> bool {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let fd_table = inner.fd_table.clone();

    let va = addr.into();
    let heap_base = inner.heap_base.0;
    let heap_end = inner.heap_end.0;
    let mmap_area_base = inner.mmap_area_base.0;
    let mmap_area_end = inner.mmap_area_end.0;

    if heap_base <= addr && addr < heap_end {
        inner.memory_set.lazy_alloc_heap(va)
    } else if mmap_area_base <= addr && addr < mmap_area_end  {
        inner.memory_set.lazy_alloc_mmap_area(va, fd_table)
    } else {
        false
    }
}