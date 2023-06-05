//! 为 qemu-system-riscv32/64 提供 SiFive 测试设备定义。

#![no_std]
#![deny(warnings, missing_docs)]

use core::cell::UnsafeCell;

/// SiFive 测试设备。
#[repr(transparent)]
pub struct SifiveTestDevice(UnsafeCell<u32>); // 实测发现源码写的 u64 但只能写进 32 位

const FAIL: u16 = 0x3333;
const PASS: u16 = 0x5555;
const RESET: u16 = 0x7777;

impl SifiveTestDevice {
    /// 以 code 为错误码，退出进程。
    #[inline]
    pub fn fail(&self, code: u16) -> ! {
        self.write(FAIL as u32 | (code as u32) << 16)
    }

    /// 以 0 为错误码，退出进程。
    #[inline]
    pub fn pass(&self) -> ! {
        self.write(PASS as _)
    }

    /// 系统重启。
    #[inline]
    pub fn reset(&self) -> ! {
        self.write(RESET as _)
    }

    #[inline]
    fn write(&self, bits: u32) -> ! {
        unsafe { self.0.get().write_volatile(bits) };
        unreachable!()
    }
}
