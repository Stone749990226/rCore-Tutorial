//! File and filesystem-related syscalls
use crate::mm::translated_byte_buffer;
use crate::sbi::console_getchar;
use crate::task::{current_user_token, suspend_current_and_run_next};

const FD_STDIN: usize = 0;
const FD_STDOUT: usize = 1;

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    match fd {
        FD_STDOUT => {
            let buffers = translated_byte_buffer(current_user_token(), buf, len);
            for buffer in buffers {
                print!("{}", core::str::from_utf8(buffer).unwrap());
            }
            len as isize
            // let slice = unsafe { core::slice::from_raw_parts(buf, len) };
            // let str = core::str::from_utf8(slice).unwrap();
            // print!("{}", str);
            // len as isize
        },
        _ => {
            panic!("Unsupported fd in sys_write!");
        }
    }
}
// 目前仅支持从标准输入 FD_STDIN 即文件描述符 0 读入，且单次读入的长度限制为 1，即每次只能读入一个字符
pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    match fd {
        FD_STDIN => {
            assert_eq!(len, 1, "Only support len = 1 in sys_read!");
            let mut c: usize;
            loop {
                // 调用 sbi 子模块提供的从键盘获取输入的接口 console_getchar ，如果返回 0 则说明还没有输入
                c = console_getchar();
                if c == 0 {
                    // 调用 suspend_current_and_run_next 暂时切换到其他进程，等下次切换回来的时候再看看是否有输入了
                    suspend_current_and_run_next();
                    continue;
                } else {
                    break;
                }
            }
            // 获取到输入之后，我们退出循环并手动查页表将输入的字符正确的写入到应用地址空间
            let ch = c as u8;
            let mut buffers = translated_byte_buffer(current_user_token(), buf, len);
            unsafe {
                buffers[0].as_mut_ptr().write_volatile(ch);
            }
            1
        }
        _ => {
            panic!("Unsupported fd in sys_read!");
        }
    }
}
