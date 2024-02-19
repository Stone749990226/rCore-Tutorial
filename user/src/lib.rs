#![no_std]
#![feature(linkage)]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

#[macro_use]
pub mod console;
mod lang_items;
mod syscall;

use buddy_system_allocator::LockedHeap;
use syscall::*;

// 在 Rust 中可变长字符串类型 String 是基于动态内存分配的。因此本章我们还要在用户库 user_lib 中支持动态内存分配
const USER_HEAP_SIZE: usize = 16384;

static mut HEAP_SPACE: [u8; USER_HEAP_SIZE] = [0; USER_HEAP_SIZE];

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

#[alloc_error_handler]
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

// 在 lib.rs 中我们定义了用户库的入口点 _start ：
#[no_mangle]
// 使用 Rust 的宏将 _start 这段代码编译后的汇编代码中放在一个名为 .text.entry 的代码段中，方便我们在后续链接的时候调整它的位置使得它能够作为用户库的入口。
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    unsafe {
        HEAP.lock()
            .init(HEAP_SPACE.as_ptr() as usize, USER_HEAP_SIZE);
    }
    exit(main());
    // exit(main());
    // panic!("unreachable after sys_exit!");
}

// 使用 Rust 的宏将其函数符号 main 标志为弱链接
// 这样在最后链接的时候，虽然在 lib.rs 和 bin 目录下的某个应用程序都有 main 符号，但由于 lib.rs 中的 main 符号是弱链接，链接器会使用 bin 目录下的应用主逻辑作为 main 。这里我们主要是进行某种程度上的保护，如果在 bin 目录下找不到任何 main ，那么编译也能够通过，但会在运行时报错。
// 为了支持上述这些链接操作，我们需要在 lib.rs 的开头加入：#![feature(linkage)]
#[linkage = "weak"]
#[no_mangle]
fn main() -> i32 {
    panic!("Cannot find main!");
}

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf)
}
// 将syscall中的系统调用在用户库 user_lib 中进一步封装，从而更加接近在 Linux 等平台的实际系统调用接口：
pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf)
}
pub fn exit(exit_code: i32) -> ! {
    sys_exit(exit_code);
}
// yield 是 Rust 的关键字，因此我们只能将应用直接调用的接口命名为 yield_
pub fn yield_() -> isize {
    sys_yield()
}

pub fn get_time() -> isize {
    sys_get_time()
}

// pub fn sbrk(size: i32) -> isize {
//     sys_sbrk(size)
// }

pub fn getpid() -> isize {
    sys_getpid()
}
pub fn fork() -> isize {
    sys_fork()
}
pub fn exec(path: &str) -> isize {
    sys_exec(path)
}
// sys_waitpid 被封装成两个不同的 API:wait 和 waitpid
// wait 表示等待任意一个子进程结束，根据 sys_waitpid 的约定它需要传的 pid 参数为 -1 
pub fn wait(exit_code: &mut i32) -> isize {
    loop {
        // 当 sys_waitpid 返回值为 -2 ，即要等待的子进程存在但它却尚未退出的时候，我们调用 yield_ 主动交出 CPU 使用权，
        // 待下次 CPU 使用权被内核交还给它的时候再次调用 sys_waitpid 查看要等待的子进程是否退出。这样做可以减小 CPU 资源的浪费。
        match sys_waitpid(-1, exit_code as *mut _) {
            -2 => {
                yield_();
            }
            // -1 or a real pid
            exit_pid => return exit_pid,
        }
    }
}

// waitpid 则等待一个进程标识符的值为pid 的子进程结束
pub fn waitpid(pid: usize, exit_code: &mut i32) -> isize {
    loop {
        match sys_waitpid(pid as isize, exit_code as *mut _) {
            -2 => {
                yield_();
            }
            // -1 or a real pid
            exit_pid => return exit_pid,
        }
    }
}
pub fn sleep(period_ms: usize) {
    let start = sys_get_time();
    while sys_get_time() < start + period_ms as isize {
        sys_yield();
    }
}