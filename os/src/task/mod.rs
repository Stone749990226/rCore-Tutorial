//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod action;
mod context;
mod manager;
mod pid;
mod processor;
mod signal;
mod switch;

// #[allow] 是一个属性(attribute)，它用于禁止指定 lint 的警告。Lint 是 Rust 编译器提供的一种代码检查机制，用于帮助开发者发现潜在的问题或者不规范的代码风格。
// #[allow(clippy::module_inception)] 意味着允许发生 module_inception 这个 lint，而不会给出警告。
#[allow(clippy::module_inception)]
#[allow(rustdoc::private_intra_doc_links)]
mod task;

// use crate::config::MAX_APP_NUM;
// use crate::loader::{get_num_app, init_app_cx};
use crate::fs::{open_file, OpenFlags};
use crate::sbi::shutdown;
use alloc::sync::Arc;
pub use context::TaskContext;
use lazy_static::*;
use manager::fetch_task;
use manager::remove_from_pid2task;
use switch::__switch;
use task::{TaskControlBlock, TaskStatus};

pub use action::{SignalAction, SignalActions};
pub use manager::{add_task, pid2task};
pub use pid::{pid_alloc, KernelStack, PidHandle};
pub use processor::{
    current_task, current_trap_cx, current_user_token, run_tasks, schedule, take_current_task,
};
pub use signal::{SignalFlags, MAX_SIG};


/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    // 首先通过 take_current_task 来取出当前正在执行的任务，修改其进程控制块内的状态
    // There must be an application running.
    let task = take_current_task().unwrap();

    // ---- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    // ---- release current PCB
    // 随后将这个任务放入任务管理器的队尾
    // push back to ready queue.
    add_task(task);
    // 调用 schedule 函数来触发调度并切换任务
    // jump to scheduling cycle
    schedule(task_cx_ptr);


    // 注意，当仅有一个任务的时候， suspend_current_and_run_next 的效果是会继续执行这个任务
}

/// pid of usertests app in make run TEST=1
pub const IDLE_PID: usize = 0;

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next(exit_code: i32) {
    // 调用 take_current_task 来将当前进程控制块从处理器监控 PROCESSOR 中取出而不是得到一份拷贝，这是为了正确维护进程控制块的引用计数
    // take from Processor
    let task = take_current_task().unwrap();

    let pid = task.getpid();
    if pid == IDLE_PID {
        println!(
            "[kernel] Idle process exit with exit_code {} ...",
            exit_code
        );
        if exit_code != 0 {
            //crate::sbi::shutdown(255); //255 == -1 for err hint
            shutdown(true)
        } else {
            //crate::sbi::shutdown(0); //0 for success hint
            shutdown(false)
        }
    }
    // remove from pid2task
    remove_from_pid2task(task.getpid());
    // **** access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
    // 进程控制块中的状态修改为 TaskStatus::Zombie 即僵尸进程，这样它后续才能被父进程在 waitpid 系统调用的时候回收
    // Change status to Zombie
    inner.task_status = TaskStatus::Zombie;
    // 将传入的退出码 exit_code 写入进程控制块中，后续父进程在 waitpid 的时候可以收集
    // Record exit code
    inner.exit_code = exit_code;
    // do not move to its parent but under initproc

    // 将当前进程的所有子进程挂在初始进程 initproc 下面，其做法是遍历每个子进程，修改其父进程为初始进程，并加入初始进程的孩子向量中
    // ++++++ access initproc TCB exclusively
    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }
    // ++++++ release parent PCB
    // 将当前进程的孩子向量清空
    inner.children.clear();
    // 对于当前进程占用的资源进行早期回收
    // deallocate user space
    inner.memory_set.recycle_data_pages();
    drop(inner);
    // **** release current PCB
    // drop task manually to maintain rc correctly
    drop(task);
    // 调用 schedule 触发调度及任务切换，由于我们再也不会回到该进程的执行过程中，因此无需关心任务上下文的保存
    // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
}

lazy_static! {
    // 调用 TaskControlBlock::new 来创建一个进程控制块，它需要传入 ELF 可执行文件的数据切片作为参数
    ///Globle process that init user shell
    pub static ref INITPROC: Arc<TaskControlBlock> = Arc::new({
        let inode = open_file("initproc", OpenFlags::RDONLY).unwrap();
        let v = inode.read_all();
        TaskControlBlock::new(v.as_slice())
    });
}
///Add init process to the manager
pub fn add_initproc() {
    add_task(INITPROC.clone());
}

pub fn check_signals_error_of_current() -> Option<(i32, &'static str)> {
    let task = current_task().unwrap();
    let task_inner = task.inner_exclusive_access();
    // println!(
    //     "[K] check_signals_error_of_current {:?}",
    //     task_inner.signals
    // );
    task_inner.signals.check_error()
}

pub fn current_add_signal(signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    task_inner.signals |= signal;
    // println!(
    //     "[K] current_add_signal:: current task sigflag {:?}",
    //     task_inner.signals
    // );
}
fn call_kernel_signal_handler(signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    match signal {
        SignalFlags::SIGSTOP => {
            task_inner.frozen = true;
            // 清除掉接收到的信号避免它们再次被处理
            task_inner.signals ^= SignalFlags::SIGSTOP;
        }
        SignalFlags::SIGCONT => {
            if task_inner.signals.contains(SignalFlags::SIGCONT) {
                task_inner.signals ^= SignalFlags::SIGCONT;
                task_inner.frozen = false;
            }
        }
        // 对于其他的信号都按照默认的处理方式即杀死当前进程，于是将 killed 字段设置为真，这样的进程会在 Trap 返回用户态之前就通过调度切换到其他进程
        _ => {
            // println!(
            //     "[K] call_kernel_signal_handler:: current task sigflag {:?}",
            //     task_inner.signals
            // );
            task_inner.killed = true;
        }
    }
}

fn call_user_signal_handler(sig: usize, signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    // 首先检查进程是否提供了该信号的处理例程，如果没有提供的话直接忽略该信号。否则就调用信号处理例程
    let handler = task_inner.signal_actions.table[sig].handler;
    if handler != 0 {
        // user handler

        // handle flag
        task_inner.handling_sig = sig as isize;
        task_inner.signals ^= signal;

        // backup trapframe
        let mut trap_ctx = task_inner.get_trap_cx();
        task_inner.trap_ctx_backup = Some(*trap_ctx);

        // 修改 Trap 上下文的 sepc 到应用设置的例程地址使得 Trap 回到用户态之后就会跳转到例程入口并开始执行
        // modify trapframe
        trap_ctx.sepc = handler;

        // 修改 Trap 上下文的 a0 寄存器，使得信号类型能够作为参数被例程接收
        // put args (a0)
        trap_ctx.x[10] = sig;
    } else {
        // default action
        println!("[K] task/call_user_signal_handler: default action: ignore it or kill process");
    }
}
fn check_pending_signals() {
    // 最外层循环遍历所有信号
    for sig in 0..(MAX_SIG + 1) {
        let task = current_task().unwrap();
        let task_inner = task.inner_exclusive_access();
        let signal = SignalFlags::from_bits(1 << sig).unwrap();
        // 检查当前进程是否接收到了遍历到的信号（条件 1）以及该信号是否未被当前进程全局屏蔽（条件 2）
        if task_inner.signals.contains(signal) && (!task_inner.signal_mask.contains(signal)) {
            let mut masked = true;
            let handling_sig = task_inner.handling_sig;
            // 检查该信号是否未被当前正在执行的信号处理例程屏蔽（条件 3）
            if handling_sig == -1 {
                masked = false;
            } else {
                let handling_sig = handling_sig as usize;
                if !task_inner.signal_actions.table[handling_sig]
                    .mask
                    .contains(signal)
                {
                    masked = false;
                }
            }
            // 当 3 个条件全部满足的时候，开始处理该信号
            if !masked {
                drop(task_inner);
                drop(task);
                // 目前的设计是：如果信号类型为 SIGKILL/SIGSTOP/SIGCONT/SIGDEF 四者之一，则该信号只能由内核来处理
                // 否则调用 call_user_signal_handler 函数尝试使用进程提供的信号处理例程来处理
                if signal == SignalFlags::SIGKILL
                    || signal == SignalFlags::SIGSTOP
                    || signal == SignalFlags::SIGCONT
                    || signal == SignalFlags::SIGDEF
                {
                    // signal is a kernel signal
                    call_kernel_signal_handler(signal);
                } else {
                    // signal is a user signal
                    call_user_signal_handler(sig, signal);
                    return;
                }
            }
        }
    }
}

// 在 trap_handler 完成 Trap 处理并返回用户态之前，会调用 handle_signals 函数处理当前进程此前接收到的信号
// 当进程收到 SIGSTOP 信号之后，它的执行将被暂停，等到该进程收到 SIGCONT 信号之后再继续执行
pub fn handle_signals() {
    // 这个循环的意义在于：只要进程还处于暂停且未被杀死的状态就会停留在循环中等待 SIGCONT 信号的到来。
    // 如果 frozen 为真，证明还没有收到 SIGCONT 信号，进程仍处于暂停状态
    loop {
        // check_pending_signals 会检查收到的信号并对它们进行处理，在这个过程中会更新 frozen 和 killed 字段
        check_pending_signals();
        let (frozen, killed) = {
            let task = current_task().unwrap();
            let task_inner = task.inner_exclusive_access();
            (task_inner.frozen, task_inner.killed)
        };
        if !frozen || killed {
            break;
        }
        // 循环的末尾我们调用 suspend_current_and_run_next 函数切换到其他进程期待其他进程将 SIGCONT 信号发过来
        suspend_current_and_run_next();
    }
}
// ch5之前：
// 需要一个全局的任务管理器来管理这些用任务控制块描述的应用
// pub struct TaskManager {
//     /// total number of tasks
//     num_app: usize,
//     /// use inner value to get mutable access
//     inner: UPSafeCell<TaskManagerInner>,
// }

// /// Inner of Task Manager
// pub struct TaskManagerInner {    
//     // 使用向量 Vec 来保存任务控制块
//     /// task list
//     tasks: Vec<TaskControlBlock>,
//     // tasks: [TaskControlBlock; MAX_APP_NUM],
//     /// id of current `Running` task
//     current_task: usize,
// }

// // 可重用并扩展之前初始化 TaskManager 的全局实例 TASK_MANAGER
// lazy_static! {
//     /// Global variable: TASK_MANAGER
//     pub static ref TASK_MANAGER: TaskManager = {
//         println!("init TASK_MANAGER");
//         // 使用 loader 子模块提供的 get_num_app 和 get_app_data 分别获取链接到内核的应用数量和每个应用的 ELF 文件格式的数据
//         let num_app = get_num_app();
//         println!("num_app = {}", num_app);
//         let mut tasks: Vec<TaskControlBlock> = Vec::new();
//         for i in 0..num_app {
//             tasks.push(TaskControlBlock::new(get_app_data(i), i));
//         }
//         TaskManager {
//             num_app,
//             inner: unsafe {
//                 UPSafeCell::new(TaskManagerInner {
//                     tasks,
//                     current_task: 0,
//                 })
//             },
//         }
//         // ch4前：
//         // let num_app = get_num_app();
//         // // 定义了一个可变的数组 tasks，其元素的类型为 TaskControlBlock 结构体。数组的长度是 MAX_APP_NUM
//         // let mut tasks = [
//         //     TaskControlBlock {
//         //         task_cx: TaskContext::zero_init(),
//         //         task_status: TaskStatus::UnInit,
//         //     }; 
//         //     MAX_APP_NUM
//         // ];
//         // // 如果应用是第一次被执行，内核需要在应用的任务控制块上构造一个用于第一次执行的任务上下文。
//         // for (i, task) in tasks.iter_mut().enumerate() {
//         //     // 先调用 init_app_cx 构造该任务的 Trap 上下文（包括应用入口地址和用户栈指针）并将其压入到内核栈顶
//         //     // 接着调用 TaskContext::goto_restore 来构造每个任务保存在任务控制块中的任务上下文
//         //     task.task_cx = TaskContext::goto_restore(init_app_cx(i));
//         //     task.task_status = TaskStatus::Ready;
//         // }
//         // TaskManager {
//         //     num_app,
//         //     inner: unsafe {
//         //         UPSafeCell::new(TaskManagerInner {
//         //             tasks,
//         //             current_task: 0,
//         //         })
//         //     },
//         // }
//     };
// }

// impl TaskManager {
//     /// Run the first task in task list.
//     ///
//     /// Generally, the first task in task list is an idle task (we call it zero process later).
//     /// But in ch3, we load apps statically, so the first task is a real app.
//     fn run_first_task(&self) -> ! {
//         let mut inner = self.inner.exclusive_access();
//         let next_task = &mut inner.tasks[0];
//         next_task.task_status = TaskStatus::Running;
//         let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
//         drop(inner);
//         let mut _unused = TaskContext::zero_init();
//         // before this, we should drop local variables that must be dropped manually
//         unsafe {
//             __switch(&mut _unused as *mut _, next_task_cx_ptr);
//         }
//         panic!("unreachable in run_first_task!");
//     }

//     /// Change the status of current `Running` task into `Ready`.
//     fn mark_current_suspended(&self) {
//         let mut inner = self.inner.exclusive_access();
//         let current = inner.current_task;
//         inner.tasks[current].task_status = TaskStatus::Ready;
//     }

//     /// Change the status of current `Running` task into `Exited`.
//     fn mark_current_exited(&self) {
//         let mut inner = self.inner.exclusive_access();
//         let current = inner.current_task;
//         inner.tasks[current].task_status = TaskStatus::Exited;
//     }

//     /// Find next task to run and return app id.
//     ///
//     /// In this case, we only return the first `Ready` task in task list.
//     fn find_next_task(&self) -> Option<usize> {
//         let inner = self.inner.exclusive_access();
//         let current = inner.current_task;
//         (current + 1..current + self.num_app + 1)
//             .map(|id| id % self.num_app)
//             .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
//     }

//     /// Get the current 'Running' task's token.
//     fn get_current_token(&self) -> usize {
//         let inner = self.inner.exclusive_access();
//         inner.tasks[inner.current_task].get_user_token()
//     }

//     /// Get the current 'Running' task's trap contexts.
//     fn get_current_trap_cx(&self) -> &'static mut TrapContext {
//         let inner = self.inner.exclusive_access();
//         inner.tasks[inner.current_task].get_trap_cx()
//     }
    
//     /// Change the current 'Running' task's program break
//     pub fn change_current_program_brk(&self, size: i32) -> Option<usize> {
//         let mut inner = self.inner.exclusive_access();
//         let cur = inner.current_task;
//         inner.tasks[cur].change_program_brk(size)
//     }

//     /// Switch current `Running` task to the task we have found,
//     /// or there is no `Ready` task and we can exit with all applications completed
//     fn run_next_task(&self) {
//         // 调用 find_next_task 方法尝试寻找一个运行状态为 Ready 的应用并返回其 ID，如果找到了执行下面的代码
//         if let Some(next) = self.find_next_task() {
//             let mut inner = self.inner.exclusive_access();
//             let current = inner.current_task;
//             inner.tasks[next].task_status = TaskStatus::Running;
//             inner.current_task = next;
//             // 分别拿到当前应用 current_task_cx_ptr 和即将被切换到的应用 next_task_cx_ptr 的任务上下文指针
//             let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
//             let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
//             // 在实际切换之前我们需要手动 drop 掉我们获取到的 TaskManagerInner 的来自 UPSafeCell 的借用标记。因为一般情况下它是在函数退出之后才会被自动释放，从而 TASK_MANAGER 的 inner 字段得以回归到未被借用的状态，之后可以再借用。如果不手动 drop 的话，编译器会在 __switch 返回时，也就是当前应用被切换回来的时候才 drop，这期间我们都不能修改 TaskManagerInner ，甚至不能读（因为之前是可变借用），会导致内核 panic 报错退出
//             drop(inner);
//             // before this, we should drop local variables that must be dropped manually
//             unsafe {
//                 __switch(current_task_cx_ptr, next_task_cx_ptr);
//             }
//             // go back to user mode
//         } else {
//             println!("All applications completed!");
//             shutdown(false);
//             // use crate::board::QEMUExit;
//             // crate::board::QEMU_EXIT_HANDLE.exit_success();
//         }
//     }
// }

// /// run first task
// pub fn run_first_task() {
//     TASK_MANAGER.run_first_task();
// }

// /// rust next task
// fn run_next_task() {
//     TASK_MANAGER.run_next_task();
// }

// /// suspend current task
// fn mark_current_suspended() {
//     TASK_MANAGER.mark_current_suspended();
// }

// /// exit current task
// fn mark_current_exited() {
//     TASK_MANAGER.mark_current_exited();
// }

// /// suspend current task, then run next task
// pub fn suspend_current_and_run_next() {
//     mark_current_suspended();
//     run_next_task();
// }

// /// exit current task,  then run next task
// pub fn exit_current_and_run_next() {
//     mark_current_exited();
//     run_next_task();
// }

// // 通过 current_user_token 可以获得当前正在执行的应用的地址空间的 token
// /// Get the current 'Running' task's token.
// pub fn current_user_token() -> usize {
//     TASK_MANAGER.get_current_token()
// }

// // 该应用地址空间中的 Trap 上下文很关键，内核需要访问它来拿到应用进行系统调用的参数并将系统调用返回值写回，通过 current_trap_cx 内核可以拿到它访问这个 Trap 上下文的可变引用并进行读写
// /// Get the current 'Running' task's trap contexts.
// pub fn current_trap_cx() -> &'static mut TrapContext {
//     TASK_MANAGER.get_current_trap_cx()
// }

// /// Change the current 'Running' task's program break
// pub fn change_program_brk(size: i32) -> Option<usize> {
//     TASK_MANAGER.change_current_program_brk(size)
// }
