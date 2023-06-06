mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::VPNRange;
pub use address::{PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum, align_up};
pub use frame_allocator::{frame_alloc, frame_alloc_more, frame_dealloc, FrameTracker, add_free};
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
