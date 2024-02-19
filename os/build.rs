use std::fs::{read_dir, File};
use std::io::{Result, Write};

// 读取位于 user/src/bin 中应用程序对应的执行文件，并生成 link_app.S ，按顺序保存链接进来的每个应用的名字
fn main() {
    println!("cargo:rerun-if-changed=../user/src/");
    println!("cargo:rerun-if-changed={}", TARGET_PATH);
    insert_app_data().unwrap();
}

static TARGET_PATH: &str = "../user/target/riscv64gc-unknown-none-elf/release/";

fn insert_app_data() -> Result<()> {
    let mut f = File::create("src/link_app.S").unwrap();
    let mut apps: Vec<_> = read_dir("../user/src/bin")
        .unwrap()
        .into_iter()
        .map(|dir_entry| {
            let mut name_with_ext = dir_entry.unwrap().file_name().into_string().unwrap();
            name_with_ext.drain(name_with_ext.find('.').unwrap()..name_with_ext.len());
            name_with_ext
        })
        .collect();
    apps.sort();

    writeln!(
        f,
        r#"
    .align 3
    .section .data
    .global _num_app
_num_app:
    .quad {}"#,
        apps.len()
    )?;

    for i in 0..apps.len() {
        writeln!(f, r#"    .quad app_{}_start"#, i)?;
    }
    writeln!(f, r#"    .quad app_{}_end"#, apps.len() - 1)?;

    // 按照顺序将各个应用的名字通过 .string 伪指令放到数据段中，注意链接器会自动在每个字符串的结尾加入分隔符 \0 ，它们的位置则由全局符号 _app_names 指出
    writeln!(
        f,
        r#"
    .global _app_names
_app_names:"#
    )?;
    for app in apps.iter() {
        writeln!(f, r#"    .string "{}""#, app)?;
    }

    for (idx, app) in apps.iter().enumerate() {
        println!("app_{}: {}", idx, app);
        // 在链接每个 ELF 执行文件之前加入一行 .align 3 来确保它们对齐到 8 字节，如果不这样做，xmas-elf crate 可能会在解析ELF的时候进行不对齐的内存读写，
        // 例如使用 ld 指令从内存的一个没有对齐到 8 字节的地址加载一个 64 位的值到一个通用寄存器
        // .incbin 是一个汇编器的伪指令（pseudo-instruction），它的作用是将一个二进制文件的内容直接嵌入到汇编代码中
        writeln!(
            f,
            r#"
    .section .data
    .global app_{0}_start
    .global app_{0}_end
    .align 3
app_{0}_start:
    .incbin "{2}{1}"
app_{0}_end:"#,
            idx, app, TARGET_PATH
        )?;
    }
    
    Ok(())
}
