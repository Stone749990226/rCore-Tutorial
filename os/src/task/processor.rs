//!Implementation of [`Processor`] and Intersection of control flow
// Processor 有一个不同的 idle 控制流，它运行在这个 CPU 核的启动栈上，功能是尝试从任务管理器中选出一个任务来在当前 CPU 核上执行。
// 在内核初始化完毕之后，会通过调用 run_tasks 函数来进入 idle 控制流
use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use lazy_static::*;
///Processor management structure
pub struct Processor {
    // 在当前处理器上正在执行的任务
    ///The task currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,
    // 当前处理器上的 idle 控制流的任务上下文
    ///The basic control flow of each core, helping to select and switch process
    idle_task_cx: TaskContext,
}

impl Processor {
    ///Create an empty Processor
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }
    // 将 self.idle_task_cx 的可变引用转换为一个指向 TaskContext 的原始指针 (*mut TaskContext)。这通常用于需要传递指针而不是引用的情况
    ///Get mutable reference to `idle_task_cx`
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        // 这里的 _ 是一个占位符，表示编译器会根据上下文自动推断指针的类型。在这种情况下，编译器会将其推断为 *mut TaskContext 类型
        &mut self.idle_task_cx as *mut _
    }
    // 取出当前正在执行的任务
    ///Get current task in moving semanteme
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        // Takes the value out of the option, leaving a None in its place
        self.current.take()
    }
    // 返回当前执行的任务的一份拷贝
    ///Get current task in cloning semanteme
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

// Processor 是描述CPU 执行状态 的数据结构。在单核CPU环境下，我们仅创建单个 Processor 的全局实例 PROCESSOR
lazy_static! {
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}
///The main part of process execution and scheduling
///Loop `fetch_task` to get the process that needs to run, and switch the process through `__switch`
pub fn run_tasks() {
    // 循环调用 fetch_task 直到顺利从任务管理器中取出一个任务，随后便准备通过任务切换的方式来执行
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            // 手动回收对即将执行任务的任务控制块的借用标记，使得后续我们仍可以访问该任务控制块。这里我们不能依赖编译器在 if let 块结尾时的自动回收，
            // 因为中间我们会在自动回收之前调用 __switch ，这将导致我们在实际上已经结束访问却没有进行回收的情况下切换到下一个任务，
            // 最终可能违反 UPSafeCell 的借用约定而使得内核报错退出
            drop(task_inner);
            // 修改当前 Processor 正在执行的任务为我们取出的任务。相当于 Arc<TaskControlBlock> 形式的任务从任务管理器流动到了处理器管理结构中。
            // 也就是说，在稳定的情况下，每个尚未结束的进程的任务控制块都只能被引用一次，要么在任务管理器中，要么则是在代表 CPU 处理器的 Processor 中
            processor.current = Some(task);
            // 同理手动回收 PROCESSOR 的借用标记
            // release processor manually
            drop(processor);
            // 调用 __switch 来从当前的 idle 控制流切换到接下来要执行的任务
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        }
    }
}
// 下面这两个函数是对 Processor::take_current/current 进行封装并提供给内核其他子模块的接口
///Take the current task,leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}
///Get running task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}
///Get token of the address space of current task
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    let token = task.inner_exclusive_access().get_user_token();
    token
}
///Get the mutable reference to trap context of current task
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}
// 当一个应用用尽了内核本轮分配给它的时间片或者它主动调用 yield 系统调用交出 CPU 使用权之后，内核会调用 schedule 函数来切换到 idle 控制流并开启新一轮的任务调度
// 传入即将被切换出去的任务的 task_cx_ptr 来在合适的位置保存任务上下文，之后就可以通过 __switch 来切换到 idle 控制流
///Return to idle control flow for new scheduling
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}
