#![allow(unused)]

use core::arch::asm;

// 定义 RustSBI 支持的服务类型常量
const SBI_SET_TIMER:                usize = 0;
const SBI_CONSOLE_PUTCHAR:          usize = 1;
const SBI_CONSOLE_GETCHAR:          usize = 2;
const SBI_CLEAR_IPI:                usize = 3;
const SBI_SEND_IPI:                 usize = 4;
const SBI_REMOTE_FENCE_I:           usize = 5;
const SBI_REMOTE_SFENCE_VMA:        usize = 6;
const SBI_REMOTE_SFENCE_VMA_ASID:   usize = 7;
const SBI_SHUTDOWN:                 usize = 8;

// which 表示请求 RustSBI 的服务的类型（RustSBI 可以提供多种不同类型的服务）
// arg0 ~ arg2 表示传递给 RustSBI 的 3 个参数
// RustSBI 在将请求处理完毕后，会给内核一个返回值，这个返回值也会被 sbi_call 函数返回
#[inline(always)]
fn sbi_call(which: usize, arg0: usize, arg1: usize, arg2: usize) -> usize {
    let mut ret;
    unsafe {
        asm!(
            "li x16, 0",
            "ecall",
            inlateout("x10") arg0 => ret,
            in("x11") arg1,
            in("x12") arg2,
            in("x17") which,
        );
    }
    ret
}

//  用来在屏幕上输出一个字符
/// use sbi call to putchar in console (qemu uart handler)
pub fn console_putchar(c: usize) {
    sbi_call(SBI_CONSOLE_PUTCHAR, c, 0, 0);
}

/// use sbi call to getchar from console (qemu uart handler)
pub fn console_getchar() -> usize {
    sbi_call(SBI_CONSOLE_GETCHAR, 0, 0, 0)
}

use crate::board::QEMUExit;
/// use sbi call to shutdown the kernel
pub fn shutdown() -> ! {
    crate::board::QEMU_EXIT_HANDLE.exit_success();

    //panic!("It should shutdown!");
}
