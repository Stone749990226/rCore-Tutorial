//! Memory management implementation
//!
//! SV39 page-based virtual-memory architecture for RV64 systems, and
//! everything about memory management, like frame allocator, page table,
//! map area and memory set, is implemented here.
//!
//! Every task or process has a memory_set to control its virtual memory.

mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

use address::VPNRange;
pub use address::{PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
pub use frame_allocator::{frame_alloc, frame_dealloc, FrameTracker};
pub use memory_set::remap_test;
pub use memory_set::{kernel_token, MapPermission, MemorySet, KERNEL_SPACE};
use page_table::PTEFlags;
pub use page_table::{
    translated_byte_buffer, translated_ref, translated_refmut, translated_str, PageTable,
    PageTableEntry, UserBuffer, UserBufferIterator,
};

/// initiate heap allocator, frame allocator and kernel space
pub fn init() {
    // 最先进行了全局动态内存分配器的初始化，因为接下来马上就要用到 Rust 的堆数据结构
    heap_allocator::init_heap();
    // 初始化物理页帧管理器（内含堆数据结构 Vec<T> ）使能可用物理页帧的分配和回收能力
    frame_allocator::init_frame_allocator();
    // 创建内核地址空间并让 CPU 开启分页模式， MMU 在地址转换的时候使用内核的多级页表
    KERNEL_SPACE.exclusive_access().activate();
}
