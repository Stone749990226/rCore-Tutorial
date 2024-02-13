//! RISC-V timer-related functionality

use crate::config::CLOCK_FREQ;
use crate::sbi::set_timer;
use riscv::register::time;

const TICKS_PER_SEC: usize = 100;
const MSEC_PER_SEC: usize = 1000;

// RISC-V 架构要求处理器要有一个内置时钟，其频率一般低于 CPU 主频。此外，还有一个计数器用来统计处理器自上电以来经过了多少个内置时钟的时钟周期。
// 在 RISC-V 64 架构上，该计数器保存在一个 64 位的 CSR mtime 中，我们无需担心它的溢出问题，在内核运行全程可以认为它是一直递增的。
/// read the `mtime` register
// timer 子模块的 get_time 函数可以取得当前 mtime 计数器的值
pub fn get_time() -> usize {
    time::read()
}

// 可以用来统计一个应用的运行时长
/// get current time in milliseconds
pub fn get_time_ms() -> usize {
    time::read() / (CLOCK_FREQ / MSEC_PER_SEC)
    // 以微秒为单位返回当前计数器的值
}

/// set the next timer interrupt
// 对 set_timer 进行了封装，它首先读取当前 mtime 的值，然后计算出 10ms 之内计数器的增量，再将 mtimecmp 设置为二者的和。这样，10ms 之后一个 S 特权级时钟中断就会被触发
pub fn set_next_trigger() {
    // CLOCK_FREQ 除以常数 TICKS_PER_SEC 即是下一次时钟中断的计数器增量值
    set_timer(get_time() + CLOCK_FREQ / TICKS_PER_SEC);
}
