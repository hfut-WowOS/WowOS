#![no_std]
#![no_main]
#![feature(panic_info_message)]

use core::arch::global_asm;
use crate::sbi::shutdown;

#[macro_use]
mod console;
mod lang_items;
mod sbi;

#[path = "boards/qemu.rs"]
mod board;

global_asm!(include_str!("entry.asm"));

// 通过宏将 rust_main 标记为 #[no_mangle] 以避免编译器对它的名字进行混淆
#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    println!("\x1b[32mHello world\x1b[0m");
    println!("\x1b[31mI'm Qi Ming\x1b[199m");
    
    shutdown();
}

// 尝试从其他地方找到全局符号 sbss 和 ebss ，它们由链接脚本 linker.ld 给出
// 分别指出需要被清零的 .bss 段的起始和终止地址
// 接下来遍历该地址区间并逐字节进行清零
// extern “C” 可以引用一个外部的 C 函数接口
/// clear BSS segment
fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    (sbss as usize..ebss as usize).for_each(|a| {
        unsafe {
            (a as *mut u8).write_volatile(0)
        }
    });
}