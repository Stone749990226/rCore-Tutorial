/* 设置了目标平台为 riscv */
OUTPUT_ARCH(riscv)
/* 设置了整个程序的入口点为之前定义的全局符号 _start */
ENTRY(_start)
/* 定义了一个常量 BASE_ADDRESS 为 0x80200000 ，也就是我们之前提到内核的初始化代码被放置的地址 */
BASE_ADDRESS = 0x80200000;

/* 链接过程中对输入的目标文件的段的合并 */
SECTIONS
{
    /* . 表示当前地址，也就是链接器会从它指向的位置开始往下放置从输入的目标文件中收集来的段。
    我们可以对 . 进行赋值来调整接下来的段放在哪里，也可以创建一些全局符号赋值为 . 从而记录这一时刻的位置 */
    . = BASE_ADDRESS;
    skernel = .;

    stext = .;
    /* 冒号前面表示最终生成的可执行文件的一个段的名字，花括号内按照放置顺序描述将所有输入目标文件的哪些段放在这个段中，每一行格式为 <ObjectFile>(SectionName)，表示目标文件 ObjectFile 的名为 SectionName 的段需要被放进去。 */
    .text : {
        /* 将包含内核第一条指令的 .text.entry 段放在最终的 .text 段的最开头，同时注意到在最终内存布局中代码段 .text 又是先于任何其他段的。因为所有的段都从 BASE_ADDRESS 也即 0x80200000 开始放置，这就能够保证内核的第一条指令正好放在 0x80200000 从而能够正确对接到 Qemu 上 */
        *(.text.entry)
        *(.text .text.*)
    }

    . = ALIGN(4K);
    etext = .;
    srodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    . = ALIGN(4K);
    erodata = .;
    sdata = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    . = ALIGN(4K);
    edata = .;
    .bss : {
        *(.bss.stack)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }

    . = ALIGN(4K);
    ebss = .;
    ekernel = .;

    /DISCARD/ : {
        *(.eh_frame)
    }
}