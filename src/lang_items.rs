// os/src/lang_items.rs
use core::panic::PanicInfo;
use crate::sbi::shutdown;

// 通过 #[panic_handler] 属性通知编译器用panic函数来对接 panic! 宏
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        println!(
            "[kernel] Panicked at {}:{} {}",
            location.file(),
            location.line(),
            info.message().unwrap()
        );
    } else {
        println!("\x1b[31m[kernel] Panicked: {}\x1b[0m", info.message().unwrap());
    }
    shutdown()
}