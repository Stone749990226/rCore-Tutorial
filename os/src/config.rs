//! Constants used in rCore
// config 子模块用来存放内核中所有的常数
// pub const USER_STACK_SIZE: usize = 4096;
// pub const KERNEL_STACK_SIZE: usize = 4096 * 2;
// pub const MAX_APP_NUM: usize = 4;
// pub const APP_BASE_ADDRESS: usize = 0x80400000;
// pub const APP_SIZE_LIMIT: usize = 0x20000;

// pub use crate::board::CLOCK_FREQ;

pub const USER_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_STACK_SIZE: usize = 4096 * 2;
pub const KERNEL_HEAP_SIZE: usize = 0x20_0000;

pub const PAGE_SIZE: usize = 0x1000;
pub const PAGE_SIZE_BITS: usize = 0xc;

pub const TRAMPOLINE: usize = usize::MAX - PAGE_SIZE + 1;
pub const TRAP_CONTEXT: usize = TRAMPOLINE - PAGE_SIZE;

pub use crate::board::{CLOCK_FREQ, MEMORY_END, MMIO};
