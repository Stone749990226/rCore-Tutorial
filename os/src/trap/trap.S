.altmacro

.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm

.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm

# 使用 .align 将 __alltraps 的地址 4 字节对齐，这是 RISC-V 特权级规范的要求
    .section .text.trampoline
    .globl __alltraps
    .globl __restore
    .align 2
__alltraps:
    # csrrw rd, csr, rs将 CSR 当前的值读到通用寄存器rd中,后将通用寄存器rs的值写入该 CSR.因此这里起到的是交换 sscratch 和 sp 的效果.在这一行之前 sp 指向用户栈， sscratch 指向内核栈，现在 sp 指向内核栈， sscratch 指向用户栈。
    csrrw sp, sscratch, sp
    # now sp->kernel stack, sscratch->user stack
    # allocate a TrapContext on kernel stack
    # 下面这一行在ch4注释掉了
    # addi sp, sp, -34*8
    # save general-purpose registers
    sd x1, 1*8(sp)
    # skip sp(x2), we will save it later
    sd x3, 3*8(sp)
    # skip tp(x4), application does not use it
    # save x5~x31
    .set n, 5
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr
    # we can use t0/t1/t2 freely, because they were saved on kernel stack
    # csrr rd csr的命令是将csr读到rd中。这里不用担心 t0 和 t1 被覆盖，因为它们刚刚已经被保存了。
    csrr t0, sstatus
    csrr t1, sepc
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)
    # read user stack from sscratch and save it on the kernel stack
    csrr t2, sscratch
    sd t2, 2*8(sp)
    # load kernel_satp into t0 将内核地址空间的 token 载入到 t0 寄存器中
    ld t0, 34*8(sp)
    # load trap_handler into t1 将 trap handler 入口点的虚拟地址载入到 t1 寄存器中
    ld t1, 36*8(sp)
    # move to kernel_sp 直接将 sp 修改为应用内核栈顶的地址
    ld sp, 35*8(sp)
    # switch to kernel space 将 satp 修改为内核地址空间的 token 并使用 sfence.vma 刷新快表，这就切换到了内核地址空间
    csrw satp, t0
    sfence.vma
    # jump to trap_handler 通过 jr 指令跳转到 t1 寄存器所保存的trap handler 入口点的地址
    jr t1
    # 解释为何我们在 __alltraps 中需要借助寄存器 jr 而不能直接 call trap_handler 了。
    # 因为在内存布局中，这条 .text.trampoline 段中的跳转指令和 trap_handler 都在代码段之内，汇编器（Assembler）和链接器（Linker）会根据 linker-qemu/k210.ld 的地址布局描述，设定跳转指令的地址，并计算二者地址偏移量，让跳转指令的实际效果为当前 pc 自增这个偏移量。
    # 但实际上由于我们设计的缘故，这条跳转指令在被执行的时候，它的虚拟地址被操作系统内核设置在地址空间中的最高页面之内，所以加上这个偏移量并不能正确的得到 trap_handler 的入口地址
    # 问题的本质可以概括为：跳转指令实际被执行时的虚拟地址和在编译器/汇编器/链接器进行后端代码生成和链接形成最终机器码时设置此指令的地址是不同的
    # # set input argument of trap_handler(cx: &mut TrapContext)
    # mv a0, sp
    # call trap_handler

# 当内核将 Trap 处理完毕准备返回用户态的时候会 调用 __restore，它有两个参数：第一个是 Trap 上下文在应用地址空间中的位置，这个对于所有的应用来说都是相同的，在 a0 寄存器中传递；第二个则是即将回到的应用的地址空间的 token ，在 a1 寄存器中传递
__restore:
    # a0: *TrapContext in user space(Constant); a1: user space token
    # switch to user space 先切换回应用地址空间（注：Trap 上下文是保存在应用地址空间中）
    csrw satp, a1
    sfence.vma
    csrw sscratch, a0 // 将传入的 Trap 上下文位置保存在 sscratch 寄存器中，这样 __alltraps 中才能基于它将 Trap 上下文保存到正确的位置
    # 将 sp 修改为 Trap 上下文的位置，后面基于它恢复各通用寄存器和 CSR
    mv sp, a0
    # now sp points to TrapContext in user space, start restoring based on it
    # restore sstatus/sepc
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    # restore general purpose registers except x0/sp/tp
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr
    # back to user stack
    ld sp, 2*8(sp)
    sret // 通过 sret 指令返回用户态

    # ch4之前的代码：
    # # case1: start running app by __restore
    # # case2: back to U after handling trap
    # # 不再需要 在开头 mv sp, a0 了。因为在 __switch 之后，sp 就已经正确指向了我们需要的 Trap 上下文地址
    # # mv sp, a0
    # # now sp->kernel stack(after allocated), sscratch->user stack
    # # restore sstatus/sepc
    # ld t0, 32*8(sp)
    # ld t1, 33*8(sp)
    # ld t2, 2*8(sp)
    # csrw sstatus, t0
    # csrw sepc, t1
    # csrw sscratch, t2
    # # restore general-purpuse registers except sp/tp
    # ld x1, 1*8(sp)
    # ld x3, 3*8(sp)
    # .set n, 5
    # .rept 27
    #     LOAD_GP %n
    #     .set n, n+1
    # .endr
    # # release TrapContext on kernel stack
    # addi sp, sp, 34*8
    # # now sp->kernel stack, sscratch->user stack
    # csrrw sp, sscratch, sp
    # sret