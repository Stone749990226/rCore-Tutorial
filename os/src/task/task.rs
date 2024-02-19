//!Implementation of [`TaskControlBlock`]
use super::TaskContext;
use super::{pid_alloc, KernelStack, PidHandle};
use crate::config::TRAP_CONTEXT;
use crate::mm::{MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use crate::trap::{trap_handler, TrapContext};
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::cell::RefMut;

// 一旦引入了任务切换机制就没有那么简单了。在一段时间内，内核需要管理多个未完成的应用，而且我们不能对应用完成的顺序做任何假定，并不是先加入的应用就一定会先完成。这种情况下，我们必须在内核中对每个应用分别维护它的运行状态
// 通过 #[derive(...)] 可以让编译器为你的类型提供一些 Trait 的默认实现。
// 实现了 Clone Trait 之后就可以调用 clone 函数完成拷贝；
// 实现了 PartialEq Trait 之后就可以使用 == 运算符比较该类型的两个实例，从逻辑上说只有 两个相等的应用执行状态才会被判为相等，而事实上也确实如此。
// Copy 是一个标记 Trait，决定该类型在按值传参/赋值的时候采用移动语义还是复制语义。
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Zombie,
}

pub struct TaskControlBlock {
    // 在初始化之后就不再变化的元数据：直接放在任务控制块中
    // immutable
    pub pid: PidHandle,
    pub kernel_stack: KernelStack,
    // 在运行过程中可能发生变化的元数据：则放在 TaskControlBlockInner 中，将它再包裹上一层 UPSafeCell<T> 放在任务控制块中。这是因为在我们的设计中外层只能获取任务控制块的不可变引用，若想修改里面的部分内容的话这需要 UPSafeCell<T> 所提供的内部可变性
    // mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

pub struct TaskControlBlockInner {
    // 位于应用地址空间次高页的 Trap 上下文被实际存放在物理页帧的物理页号 trap_cx_ppn, 它能够方便我们对于 Trap 上下文进行访问
    pub trap_cx_ppn: PhysPageNum,
    // base_size 统计了应用数据的大小，也就是在应用地址空间中从 0x0 开始到用户栈结束一共包含多少字节。应用数据仅有可能出现在应用地址空间低于 base_size 字节的区域中。借助它我们可以清楚的知道应用有多少数据驻留在内存中
    pub base_size: usize,
    // 将暂停的任务的任务上下文保存在任务控制块中
    pub task_cx: TaskContext,
    // 当前进程的执行状态
    pub task_status: TaskStatus,
    // 应用的地址空间 memory_set
    pub memory_set: MemorySet,
    // 在维护父子进程关系的时候大量用到了引用计数 Arc/Weak 。进程控制块的本体是被放到内核堆上面的，对于它的一切访问都是通过智能指针 Arc/Weak 来进行的，这样是便于建立父子进程的双向链接关系（避免仅基于 Arc 形成环状链接关系）。
    // 当且仅当智能指针 Arc 的引用计数变为 0 的时候，进程控制块以及被绑定到它上面的各类资源才会被回收
    // parent 指向当前进程的父进程（如果存在的话）。注意我们使用 Weak 而非 Arc 来包裹另一个任务控制块，因此这个智能指针将不会影响父进程的引用计数
    pub parent: Option<Weak<TaskControlBlock>>,
    // children 则将当前进程的所有子进程的任务控制块以 Arc 智能指针的形式保存在一个向量中，这样才能够更方便的找到它们
    pub children: Vec<Arc<TaskControlBlock>>,
    // 进程调用 exit 系统调用主动退出或者执行出错由内核终止的时候，它的退出码 exit_code 会被内核保存在它的任务控制块中，并等待它的父进程通过 waitpid 回收它的资源的同时也收集它的 PID 以及退出码
    pub exit_code: i32,
    // 应用动态内存分配的堆空间的大小
    // pub heap_bottom: usize,
    // pub program_brk: usize,
}

impl TaskControlBlockInner {
    /*
    pub fn get_task_cx_ptr2(&self) -> *const usize {
        &self.task_cx_ptr as *const usize
    }
    */
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }
}

impl TaskControlBlock {
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    // new 用来创建一个新的进程，目前仅用于内核中手动创建唯一一个初始进程 initproc
    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        // 解析应用的 ELF 执行文件得到应用地址空间 memory_set ，用户栈在应用地址空间中的位置 user_sp 以及应用的入口点 entry_point
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        // 手动查页表找到位于应用地址空间中新创建的Trap 上下文被实际放在哪个物理页帧上，用来做后续的初始化
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        // 为该进程分配 PID 以及内核栈，并记录下内核栈在内核地址空间的位置 kernel_stack_top
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        // push a task context which goes to trap_return to the top of kernel stack
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                })
            },
        };
        // 初始化位于该进程应用地址空间中的 Trap 上下文，使得第一次进入用户态的时候时候能正确跳转到应用入口点并设置好用户栈，同时也保证在 Trap 的时候用户态能正确进入内核态
        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }
    // exec 用来实现 exec 系统调用，即当前进程加载并执行另一个 ELF 格式可执行文件
    pub fn exec(&self, elf_data: &[u8]) {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();

        // **** access inner exclusively
        let mut inner = self.inner_exclusive_access();
        // 从 ELF 文件生成一个全新的地址空间并直接替换进来，这将导致原有的地址空间生命周期结束，里面包含的全部物理页帧都会被回收
        // substitute memory_set
        inner.memory_set = memory_set;
        // update trap_cx ppn
        inner.trap_cx_ppn = trap_cx_ppn;
        // initialize base_size
        inner.base_size = user_sp;
        // 修改新的地址空间中的 Trap 上下文，将解析得到的应用入口点、用户栈位置以及一些内核的信息进行初始化，这样才能正常实现 Trap 机制
        // initialize trap_cx
        let trap_cx = inner.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );

        // 无需对任务上下文进行处理，因为这个进程本身已经在执行了，而只有被暂停的应用才需要在内核栈上保留一个任务上下文
        
        // **** release inner automatically
    }
    // fork 用来实现 fork 系统调用，即当前进程 fork 出来一个与之几乎相同的子进程
    // 实现方法基本上和新建进程控制块的 TaskControlBlock::new 是相同的
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        // ---- access parent PCB exclusively
        let mut parent_inner = self.inner_exclusive_access();
        // 子进程的地址空间不是通过解析 ELF 文件，而是调用 MemorySet::from_existed_user 复制父进程地址空间得到的
        // copy user space(include trap context)
        let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        // alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    // 在子进程内核栈上压入一个初始化的任务上下文，使得内核一旦通过任务切换到该进程，就会跳转到 trap_return 来进入用户态
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    // fork 的时候需要注意父子进程关系的维护。将父进程的弱引用计数放到子进程的进程控制块中
                    // self 是一个 Arc<Self> 类型，表示对当前的进程控制块的强引用。Arc::downgrade 方法将这个强引用转换为一个弱引用
                    // 弱引用的作用是避免形成循环引用。在父进程拥有子进程的强引用的同时，子进程也拥有父进程的强引用，如果两者之间存在强引用，就会形成循环引用，导致内存泄漏。因此，将父进程的强引用转换为弱引用，可以避免这种情况的发生
                    // 在需要使用父进程时，可以通过弱引用尝试获取其强引用，如果父进程已经被销毁，则获取到的结果会是 None
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                })
            },
        });
        // 将子进程插入到父进程的孩子向量 children 中
        // add child
        parent_inner.children.push(task_control_block.clone());
        // modify kernel_sp in trap_cx
        // **** access children PCB exclusively
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        // return
        task_control_block
        // ---- release parent PCB automatically
        // **** release children PCB automatically
    }
    // 以 usize 的形式返回当前进程的进程标识符以 usize 的形式返回当前进程的进程标识符
    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}

// ch5之前
// impl TaskControlBlock {
//     pub fn get_trap_cx(&self) -> &'static mut TrapContext {
//         self.trap_cx_ppn.get_mut()
//     }
//     pub fn get_user_token(&self) -> usize {
//         self.memory_set.token()
//     }
//     pub fn new(elf_data: &[u8], app_id: usize) -> Self {
//         // memory_set with elf program headers/trampoline/trap context/user stack
//         let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
//         // 从地址空间 memory_set 中查多级页表找到应用地址空间中的 Trap 上下文实际被放在哪个物理页帧
//         let trap_cx_ppn = memory_set
//             .translate(VirtAddr::from(TRAP_CONTEXT).into())
//             .unwrap()
//             .ppn();
//         let task_status = TaskStatus::Ready;
//         // map a kernel-stack in kernel space 找到应用的内核栈预计放在内核地址空间 KERNEL_SPACE 中的哪个位置
//         let (kernel_stack_bottom, kernel_stack_top) = kernel_stack_position(app_id);
//         // 通过 insert_framed_area 实际将这个逻辑段 加入到内核地址空间中
//         KERNEL_SPACE.exclusive_access().insert_framed_area(
//             kernel_stack_bottom.into(),
//             kernel_stack_top.into(),
//             MapPermission::R | MapPermission::W,
//         );
//         let task_control_block = Self {
//             task_status,
//             // 在应用的内核栈顶压入一个跳转到 trap_return 而不是 __restore 的任务上下文，这主要是为了能够支持对该应用的启动并顺利切换到用户地址空间执行
//             task_cx: TaskContext::goto_trap_return(kernel_stack_top),
//             memory_set,
//             trap_cx_ppn,
//             base_size: user_sp,
//             heap_bottom: user_sp,
//             program_brk: user_sp,
//         };
//         // prepare TrapContext in user space
//         let trap_cx = task_control_block.get_trap_cx();
//         *trap_cx = TrapContext::app_init_context(
//             entry_point,
//             user_sp,
//             KERNEL_SPACE.exclusive_access().token(),
//             kernel_stack_top,
//             trap_handler as usize,
//         );
//         task_control_block
//     }
//     /// change the location of the program break. return None if failed.
//     pub fn change_program_brk(&mut self, size: i32) -> Option<usize> {
//         let old_break = self.program_brk;
//         let new_brk = self.program_brk as isize + size as isize;
//         if new_brk < self.heap_bottom as isize {
//             return None;
//         }
//         let result = if size < 0 {
//             self.memory_set
//                 .shrink_to(VirtAddr(self.heap_bottom), VirtAddr(new_brk as usize))
//         } else {
//             self.memory_set
//                 .append_to(VirtAddr(self.heap_bottom), VirtAddr(new_brk as usize))
//         };
//         if result {
//             self.program_brk = new_brk as usize;
//             Some(old_break)
//         } else {
//             None
//         }
//     }
// }