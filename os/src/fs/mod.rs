mod info;
mod inode;
mod pipe;
mod stdio;
mod mount;

use crate::mm::UserBuffer;

use alloc::sync::Arc;
pub use info::*;
pub use inode::*;
pub use pipe::{make_pipe, Pipe};
pub use stdio::{Stdin, Stdout};
pub use mount::MNT_TABLE;

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

impl File for FileDescriptor {
    fn readable(&self) -> bool {
        match self {
            FileDescriptor::OSInode(inode) => inode.readable(),
            FileDescriptor::Other(inode) => inode.readable(),
        }
    }
    
    fn writable(&self) -> bool {
        match self {
            FileDescriptor::OSInode(inode) => inode.writable(),
            FileDescriptor::Other(inode) => inode.writable(),
        }
    }
    
    fn read(&self, buf: UserBuffer) -> usize {
        match self {
            FileDescriptor::OSInode(inode) => inode.read(buf),
            FileDescriptor::Other(inode) => inode.read(buf),
        }
    }
    
    fn write(&self, buf: UserBuffer) -> usize {
        match self {
            FileDescriptor::OSInode(inode) => inode.write(buf),
            FileDescriptor::Other(inode) => inode.write(buf),
        }
    }
}

unsafe impl Sync for FileDescriptor {}

unsafe impl Send for FileDescriptor {}