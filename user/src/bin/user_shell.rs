#![no_std]
#![no_main]
#![allow(clippy::println_empty_string)]

extern crate alloc;

#[macro_use]
extern crate user_lib;

const LF: u8 = 0x0au8;
const CR: u8 = 0x0du8;
const DL: u8 = 0x7fu8;
const BS: u8 = 0x08u8;

use alloc::string::String;
use user_lib::console::getchar;
use user_lib::{exec, fork, waitpid};

#[no_mangle]
pub fn main() -> i32 {
    println!("Rust user shell");
    let mut line: String = String::new();
    print!(">> ");
    loop {
        let c = getchar();
        match c {
            // 如果用户输入的是换行符（LF）或回车符（CR），表示用户输入了一行完整的命令
            LF | CR => {
                println!("");
                // 如果用户输入的命令不为空
                if !line.is_empty() {
                    // 在命令末尾添加一个空字符，以便后续调用 exec() 函数执行该命令
                    line.push('\0');
                    let pid = fork();
                    if pid == 0 {
                        // child process
                        if exec(line.as_str()) == -1 {
                            println!("Error when executing!");
                            return -4;
                        }
                        unreachable!();
                    } else {
                        let mut exit_code: i32 = 0;
                        let exit_pid = waitpid(pid as usize, &mut exit_code);
                        assert_eq!(pid, exit_pid);
                        println!("Shell: Process {} exited with code {}", pid, exit_code);
                    }
                    // 清空命令行，准备接收下一条命令
                    line.clear();
                }
                // 打印 Shell 的提示符，表示用户可以继续输入命令
                print!(">> ");
            }
            // 如果用户输入的是退格符（BS）或者删除符（DL），表示用户要删除输入的字符
            BS | DL => {
                if !line.is_empty() {
                    // 退格
                    print!("{}", BS as char);
                    // 打印一个空格，覆盖原来的字符
                    print!(" ");
                    // 再次退格，让光标回退到原来位置
                    print!("{}", BS as char);
                    // 删除 line 中最后一个字符
                    line.pop();
                }
            }
            _ => {
                // 对于其他字符，直接打印该字符，并将其添加到命令行中
                print!("{}", c as char);
                line.push(c as char);
            }
        }
    }
}
