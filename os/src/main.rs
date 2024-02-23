// os/src/main.rs
#![no_std]
#![no_main]
// 需要在 main.rs 开头加上 #![feature(panic_info_message)] 才能通过 PanicInfo::message 获取报错信息
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

// 需要引入 alloc 库的依赖，由于它算是 Rust 内置的 crate ，我们并不是在 Cargo.toml 中进行引入，而是在 main.rs 中声明即可
extern crate alloc;

#[macro_use]
extern crate bitflags;

#[path = "boards/qemu.rs"]
mod board;
// console_putchar 的功能过于受限，如果想打印一行 Hello world! 的话需要进行多次调用。能否像本章第一节那样使用 println! 宏一行就完成输出呢？因此我们尝试自己编写基于 console_putchar 的 println! 宏。
#[macro_use]
mod console;
mod config;
mod drivers;
pub mod fs;
pub mod lang_items;
// pub mod loader;
pub mod mm;
pub mod sbi;
// 第二章专属模块，后面弃用
// pub mod batch;
pub mod sync;
pub mod syscall;
pub mod task;
// ch3引入模块
pub mod timer;
pub mod trap;

// use log::*;
// mod logging;
use core::arch::global_asm;
// 在 main.rs 中嵌入这段汇编代码，这样 Rust 编译器才能够注意到entry.asm，不然编译器会认为它是一个与项目无关的文件
// 通过 include_str! 宏将同目录下的汇编代码 entry.asm 转化为字符串并通过 global_asm! 宏嵌入到代码中
global_asm!(include_str!("entry.asm"));
global_asm!(include_str!("link_app.S"));

// 这里需要注意的是需要通过宏将 rust_main 标记为 #[no_mangle] 以避免编译器对它的名字进行混淆，不然在链接的时候， entry.asm 将找不到 main.rs 提供的外部符号 rust_main 从而导致链接失败
#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    println!("[kernel] Hello, world!");
    mm::init();
    mm::remap_test();
    trap::init();
    //trap::enable_interrupt();
    trap::enable_timer_interrupt();
    timer::set_next_trigger();
    fs::list_apps();
    task::add_initproc();
    task::run_tasks();
    panic!("Unreachable in rust_main!");
}

// 在内核初始化中，需要先完成对 .bss 段的清零。这是内核很重要的一部分初始化工作，在使用任何被分配到 .bss 段的全局变量之前我们需要确保 .bss 段已被清零。我们就在 rust_main 的开头完成这一工作，由于控制权已经被转交给 Rust ，我们终于不用手写汇编代码而是可以用 Rust 来实现这一功能了：
fn clear_bss() {
    // extern “C” 可以引用一个外部的 C 函数接口（这意味着调用它的时候要遵从目标平台的 C 语言调用规范）。但我们这里只是引用位置标志并将其转成 usize 获取它的地址。由此可以知道 .bss 段两端的地址。
    extern "C" {
        // fn stext(); // begin addr of text segment
        // fn etext(); // end addr of text segment
        // fn srodata(); // start addr of Read-Only data segment
        // fn erodata(); // end addr of Read-Only data ssegment
        // fn sdata(); // start addr of data segment
        // fn edata(); // end addr of data segment
        fn sbss(); // start addr of BSS segment
        fn ebss(); // end addr of BSS segment
        // fn boot_stack_lower_bound(); // stack lower bound
        // fn boot_stack_top(); // stack top
    }
    // // 在函数 clear_bss 中，我们会尝试从其他地方找到全局符号 sbss 和 ebss ，它们由链接脚本 linker.ld 给出，并分别指出需要被清零的 .bss 段的起始和终止地址。接下来我们只需遍历该地址区间并逐字节进行清零即可。
    // (sbss as usize..ebss as usize).for_each(|a| {
    //     unsafe { (a as *mut u8).write_volatile(0) }
    // });
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}







