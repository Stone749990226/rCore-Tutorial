/// Trap Context
use riscv::register::sstatus::{self, Sstatus, SPP};

// 告诉编译器使用 C 语言风格的内存布局
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapContext {
    /// general regs[0..31]
    pub x: [usize; 32],
    /// CSR sstatus      
    pub sstatus: Sstatus,
    /// CSR sepc
    pub sepc: usize,
    /// Addr of Page Table
    pub kernel_satp: usize, // 表示内核地址空间的 token ，即内核页表的起始物理地址
    /// kernel stack
    pub kernel_sp: usize, // 表示当前应用在内核地址空间中的内核栈栈顶的虚拟地址
    /// Addr of trap_handler function
    pub trap_handler: usize, // 表示内核中 trap handler 入口点的虚拟地址
}


impl TrapContext {
    pub fn set_sp(&mut self, sp: usize) { self.x[2] = sp; }
    /// init app context
    pub fn app_init_context(
        entry: usize,
        sp: usize,
        kernel_satp: usize,
        kernel_sp: usize,
        trap_handler: usize,
    ) -> Self {
        let mut sstatus = sstatus::read(); // CSR sstatus
        sstatus.set_spp(SPP::User); //previous privilege mode: user mode
        let mut cx = Self {
            x: [0; 32],
            sstatus,
            sepc: entry,  // entry point of app
            kernel_satp,  // addr of page table
            kernel_sp,    // kernel stack
            trap_handler, // addr of trap_handler function
        };
        cx.set_sp(sp); // app's user stack pointer
        cx // return initial Trap Context of app
    }
    // ch4之前：
    // pub fn app_init_context(entry: usize, sp: usize) -> Self {
    //     let mut sstatus = sstatus::read();
    //     sstatus.set_spp(SPP::User);
    //     let mut cx = Self {
    //         x: [0; 32],
    //         sstatus,
    //         sepc: entry,
    //     };
    //     cx.set_sp(sp);
    //     cx
    // }
}