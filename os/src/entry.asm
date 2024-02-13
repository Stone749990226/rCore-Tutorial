# 要编写进入内核后的第一条指令
# os/src/entry.asm
    # .section表明我们希望将后面的内容全部放到一个名为 .text.entry 的段中
    .section .text.entry
    # .globl告知编译器 _start 是一个全局符号，因此可以被其他目标文件使用
    .globl _start
_start:
    la sp, boot_stack_top
    call rust_main

    .section .bss.stack
    .globl boot_stack_lower_bound
boot_stack_lower_bound:
    .space 4096 * 16
    .globl boot_stack_top
boot_stack_top: