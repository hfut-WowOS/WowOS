mod info;
mod inode;
mod pipe;
mod stdio;

use crate::mm::UserBuffer;

use alloc::sync::Arc;
pub use info::{dirent, stat};
pub use inode::*;
pub use pipe::{make_pipe, Pipe};
pub use stdio::{Stdin, Stdout};

// 这个接口在内存和存储设备之间建立了数据交换的通道
pub trait File: Send + Sync {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn read(&self, buf: UserBuffer) -> usize;   // UserBuffer是mm子模块中定义的应用地址空间中的一段缓冲区（即内存）的抽象
    fn write(&self, buf: UserBuffer) -> usize;
}

#[derive(Clone)]
pub enum FileDescriptor {
    OSInode(Arc<OSInode>),
    Other(Arc<dyn File + Send + Sync>),
}
