//! Implementation of [`TaskContext`]
use crate::trap::trap_return;

/// Task Context
// #[derive(Copy, Clone)]
#[repr(C)]
pub struct TaskContext {
    /// return address ( e.g. __restore ) of __switch ASM function
    // 保存 ra 很重要，它记录了 __switch 函数返回之后应该跳转到哪里继续执行，从而在任务切换完成并 ret 之后能到正确的位置
    ra: usize,
    /// kernel stack pointer of app
    sp: usize,
    /// callee saved registers:  s 0..11
    s: [usize; 12],
}

impl TaskContext {
    /// init task context
    pub fn zero_init() -> Self {
        Self {
            ra: 0,
            sp: 0,
            s: [0; 12],
        }
    }

    // 当每个应用第一次获得 CPU 使用权即将进入用户态执行的时候，它的内核栈顶放置着我们在 内核加载应用的时候 构造的一个任务上下文
    /// set Task Context{__restore ASM funciton: trap_return, sp: kstack_ptr, s: s_0..12}
    pub fn goto_trap_return(kstack_ptr: usize) -> Self {
        Self {
            // 在 __switch 切换到该应用的任务上下文的时候，内核将会跳转到 trap_return 并返回用户态开始该应用的启动执行
            ra: trap_return as usize,
            sp: kstack_ptr,
            s: [0; 12],
        }
    }
    // ch4之前：
    // pub fn goto_restore(kstack_ptr: usize) -> Self {
    //     extern "C" {
    //         fn __restore();
    //     }
    //     Self {
    //         ra: __restore as usize,
    //         sp: kstack_ptr,
    //         s: [0; 12],
    //     }
    // }
}
