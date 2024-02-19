//! Trap handling functionality
//!
//! For rCore, we have a single trap entry point, namely `__alltraps`. At
//! initialization in [`init()`], we set the `stvec` CSR to point to it.
//!
//! All traps go through `__alltraps`, which is defined in `trap.S`. The
//! assembly language code does just enough work restore the kernel space
//! context, ensuring that Rust code safely runs, and transfers control to
//! [`trap_handler()`].
//!
//! It then calls different functionality based on what exactly the exception
//! was. For example, timer interrupts trigger task preemption, and syscalls go
//! to [`syscall()`].

mod context;

// use crate::batch::run_next_app;
use crate::config::{TRAMPOLINE, TRAP_CONTEXT};
use crate::syscall::syscall;
use crate::task::{
    current_trap_cx, current_user_token, exit_current_and_run_next, suspend_current_and_run_next,
};
use crate::timer::set_next_trigger;
use core::arch::{asm, global_asm};
use riscv::register::{
    mtvec::TrapMode,
    scause::{self, Exception, Interrupt, Trap},
    sie, stval, stvec,
};

global_asm!(include_str!("trap.S"));
/// initialize CSR `stvec` as the entry of `__alltraps`
pub fn init() {
    set_kernel_trap_entry();
}
fn set_kernel_trap_entry() {
    // "stvec" 寄存器存储了异常处理程序的入口地址
    unsafe {
        stvec::write(trap_from_kernel as usize, TrapMode::Direct);
    }
}
fn set_user_trap_entry() {
    unsafe {
        // 把 stvec 设置为内核和应用地址空间共享的跳板页面的起始地址 TRAMPOLINE 而不是编译器在链接时看到的 __alltraps 的地址。
        // 这是因为启用分页模式之后，内核只能通过跳板页面上的虚拟地址来实际取得 __alltraps 和 __restore 的汇编代码
        stvec::write(TRAMPOLINE as usize, TrapMode::Direct);
    }
}
// ch4之前：
// 引入了一个外部符号 __alltraps ，并将 stvec 设置为 Direct 模式指向它的地址
// pub fn init() {
//     extern "C" {
//         fn __alltraps();
//     }
//     unsafe {
//         stvec::write(__alltraps as usize, TrapMode::Direct);
//     }
// }

/// timer interrupt enabled
// 设置了 sie.stie 使得 S 特权级时钟中断不会被屏蔽
pub fn enable_timer_interrupt() {
    unsafe {
        sie::set_stimer();
    }
}

#[no_mangle]
pub fn trap_handler() -> ! {
    // 在 trap_handler 的开头还调用 set_kernel_trap_entry 将 stvec 修改为同模块下另一个函数 trap_from_kernel 的地址。这就是说，一旦进入内核后再次触发到 S态 Trap，则硬件在设置一些 CSR 寄存器之后，会跳过对通用寄存器的保存过程，直接跳转到 trap_from_kernel 函数，在这里直接 panic 退出。
    // 这里为了简单起见，弱化了 S态 –> S态的 Trap 处理过程：直接 panic 。
    set_kernel_trap_entry();
    // 由于应用的 Trap 上下文不在内核地址空间，因此我们调用 current_trap_cx 来获取当前应用的 Trap 上下文的可变引用而不是像之前那样作为参数传入 trap_handler
    let scause = scause::read();
    let stval = stval::read();
    match scause.cause() {
        Trap::Exception(Exception::UserEnvCall) => {
            // jump to next instruction anyway
            let mut cx = current_trap_cx();
            // 在调用 syscall 进行系统调用分发并具体调用 sys_fork 之前，trap_handler 已经将当前进程 Trap 上下文中的 sepc 向后移动了 4 字节，使得它回到用户态之后，会从发出系统调用的 ecall 指令的下一条指令开始执行
            cx.sepc += 4;
            // get system call return value
            let result = syscall(cx.x[17], [cx.x[10], cx.x[11], cx.x[12]]);
            // cx is changed during sys_exec, so we have to call it again
            cx = current_trap_cx();
            // 父进程系统调用的返回值会在 trap_handler 中 syscall 返回之后再设置为 sys_fork 的返回值，这里我们返回子进程的 PID
            cx.x[10] = result as usize;
        }
        Trap::Exception(Exception::StoreFault)
        | Trap::Exception(Exception::StorePageFault)
        | Trap::Exception(Exception::InstructionFault)
        | Trap::Exception(Exception::InstructionPageFault)
        | Trap::Exception(Exception::LoadFault)
        | Trap::Exception(Exception::LoadPageFault) => {
            println!(
                "[kernel] {:?} in application, bad addr = {:#x}, bad instruction = {:#x}, kernel killed it.",
                scause.cause(),
                stval,
                current_trap_cx().sepc,
            );
            // page fault exit code
            exit_current_and_run_next(-2);
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            println!("[kernel] IllegalInstruction in application, kernel killed it.");
            // illegal instruction exit code
            exit_current_and_run_next(-3);
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            set_next_trigger();
            suspend_current_and_run_next();
        }
        _ => {
            panic!(
                "Unsupported trap {:?}, stval = {:#x}!",
                scause.cause(),
                stval
            );
        }
    }
    trap_return();
}
// ch4之前：
// pub fn trap_handler(cx: &mut TrapContext) -> &mut TrapContext {
//     let scause = scause::read();
//     let stval = stval::read();
//    ···
//     cx
// }
#[no_mangle]
/// set the new addr of __restore asm function in TRAMPOLINE page,
/// set the reg a0 = trap_cx_ptr, reg a1 = phy addr of usr page table,
/// finally, jump to new addr of __restore asm function
pub fn trap_return() -> ! {
    // 在 trap_return 的开始处就调用 set_user_trap_entry ，来让应用 Trap 到 S 的时候可以跳转到 __alltraps
    set_user_trap_entry();
    // 准备好 __restore 需要两个参数：分别是 Trap 上下文在应用地址空间中的虚拟地址和要继续执行的应用地址空间的 token
    let trap_cx_ptr = TRAP_CONTEXT;
    let user_satp = current_user_token();
    // 这里 __alltraps 和 __restore 都是指编译器在链接时看到的内核内存布局中的地址。
    extern "C" {
        fn __alltraps();
        fn __restore();
    }
    // 计算 __restore 虚地址的过程：由于 __alltraps 是对齐到地址空间跳板页面的起始地址 TRAMPOLINE 上的， 则 __restore 的虚拟地址只需在 TRAMPOLINE 基础上加上 __restore 相对于 __alltraps 的偏移量即可。
    let restore_va = __restore as usize - __alltraps as usize + TRAMPOLINE;
    unsafe {
        asm!(
            // 使用 fence.i 指令清空指令缓存 i-cache 。这是因为，在内核中进行的一些操作可能导致一些原先存放某个应用代码的物理页帧如今用来存放数据或者是其他应用的代码，i-cache 中可能还保存着该物理页帧的错误快照。因此我们直接将整个 i-cache 清空避免错误
            "fence.i",
            // 使用 jr 指令完成了跳转到 __restore 的任务
            "jr {restore_va}",             // jump to new addr of __restore asm function
            restore_va = in(reg) restore_va,
            in("a0") trap_cx_ptr,      // a0 = virt addr of Trap Context
            in("a1") user_satp,        // a1 = phy addr of usr page table
            options(noreturn)
        );
    }
}


#[no_mangle]
/// Unimplement: traps/interrupts/exceptions from kernel mode
/// Todo: Chapter 9: I/O device
pub fn trap_from_kernel() -> ! {
    panic!("a trap from kernel!");
}

pub use context::TrapContext;