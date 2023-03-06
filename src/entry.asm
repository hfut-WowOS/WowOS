 # os/src/entry.asm
 # 进入内核后的第一条指令
 # _start:全局符号可以被其他目标文件使用

.section .text.entry
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