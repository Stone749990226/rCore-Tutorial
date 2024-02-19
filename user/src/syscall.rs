use core::arch::asm;
// 寄存器 a0~a6 保存系统调用的参数， a0 保存系统调用的返回值， a7 用来传递 syscall ID，这是因为所有的 syscall 都是通过 ecall 指令触发的，除了各输入参数之外我们还额外需要一个寄存器来保存要请求哪个系统调用。
// 由于这超出了 Rust 语言的表达能力，我们需要在代码中使用内嵌汇编来完成参数/返回值绑定和 ecall 指令的插入：
// 我们将所有的系统调用都封装成 syscall 函数，可以看到它支持传入 syscall ID 和 3 个参数（a0~a2寄存器中）。
fn syscall(id: usize, args: [usize; 3]) -> isize {
    let mut ret: isize;
    unsafe {
        // 我们曾经使用 global_asm! 宏来嵌入全局汇编代码，而这里的 asm! 宏可以将汇编代码嵌入到局部的函数上下文中。
        // 比 global_asm! ， asm! 宏可以获取上下文中的变量信息并允许嵌入的汇编代码对这些变量进行操作。
        asm!(
            "ecall",
            // a0 寄存器比较特殊，它同时作为输入和输出，因此我们将 in 改成 inlateout ，并在行末的变量部分使用 {in_var} => {out_var} 的格式，其中 {in_var} 和 {out_var} 分别表示上下文中的输入变量和输出变量。
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x17") id
        );
    }
    ret
}

// 于是 sys_write 和 sys_exit 只需将 syscall 进行包装：
const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_GET_TIME: usize = 169;
const SYSCALL_GETPID: usize = 172;
const SYSCALL_FORK: usize = 220;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_WAITPID: usize = 260;
// const SYSCALL_SBRK: usize = 214;

/// 功能：从文件中读取一段内容到缓冲区。
/// 参数：fd 是待读取文件的文件描述符，切片 buffer 则给出缓冲区。
/// 返回值：如果出现了错误则返回 -1，否则返回实际读到的字节数。
/// syscall ID：63
pub fn sys_read(fd: usize, buffer: &mut [u8]) -> isize {
    syscall(
        SYSCALL_READ,
        [fd, buffer.as_mut_ptr() as usize, buffer.len()],
    )
}

// sys_write 使用一个 &[u8] 切片类型来描述缓冲区，这是一个 胖指针 (Fat Pointer)，里面既包含缓冲区的起始地址，还 包含缓冲区的长度。我们可以分别通过 as_ptr 和 len 方法取出它们并独立地作为实际的系统调用参数。
pub fn sys_write(fd: usize, buffer: &[u8]) -> isize {
    syscall(SYSCALL_WRITE, [fd, buffer.as_ptr() as usize, buffer.len()])
}

pub fn sys_exit(exit_code: i32) -> ! {
    syscall(SYSCALL_EXIT, [exit_code as usize, 0, 0]);
    panic!("sys_exit never returns!");
}

pub fn sys_yield() -> isize {
    syscall(SYSCALL_YIELD, [0, 0, 0])
}

pub fn sys_get_time() -> isize {
    syscall(SYSCALL_GET_TIME, [0, 0, 0])
}

// pub fn sys_sbrk(size: i32) -> isize {
//     syscall(SYSCALL_SBRK, [size as usize, 0, 0])
// }

pub fn sys_getpid() -> isize {
    syscall(SYSCALL_GETPID, [0, 0, 0])
}

pub fn sys_fork() -> isize {
    syscall(SYSCALL_FORK, [0, 0, 0])
}

pub fn sys_exec(path: &str) -> isize {
    syscall(SYSCALL_EXEC, [path.as_ptr() as usize, 0, 0])
}

pub fn sys_waitpid(pid: isize, exit_code: *mut i32) -> isize {
    syscall(SYSCALL_WAITPID, [pid as usize, exit_code as usize, 0])
}