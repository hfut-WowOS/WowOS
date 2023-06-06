use alloc::sync::Arc;
use alloc::vec::Vec;

use fatfs::VFile;


pub use info::{Dirent, Kstat};
pub use inode::{ch_dir, list_apps, open_file, init};
pub use inode::{FileType, OpenFlags, OSInode};
pub use pipe::{make_pipe, Pipe};
pub use stdio::{Stdin, Stdout};
pub use mount::MNT_TABLE;
use crate::fs::inode::ROOT_INODE;
use crate::mm::UserBuffer;



mod info;
mod inode;
mod mount;
mod pipe;
mod stdio;

pub trait File: Send + Sync {
    fn readable(&self) -> bool;
    fn writable(&self) -> bool;
    fn read(&self, buf: UserBuffer) -> usize;
    fn write(&self, buf: UserBuffer) -> usize;
}

pub fn get_current_inode(curr_path: &str) -> Arc<VFile> {
    if curr_path == "/" || curr_path.contains("^/") {
        ROOT_INODE.clone()
    } else {
        let path: Vec<&str> = curr_path.split("/").collect();
        ROOT_INODE.find_vfile_bypath(path).unwrap()
    }
}

#[derive(Clone)]
pub enum FileDescriptor {
    Regular(Arc<OSInode>),
    Abstract(Arc<dyn File + Send + Sync>),
}

impl File for FileDescriptor {
    fn readable(&self) -> bool {
        match self {
            FileDescriptor::Regular(inode) => inode.readable(),
            FileDescriptor::Abstract(inode) => inode.readable(),
        }
    }
    
    fn writable(&self) -> bool {
        match self {
            FileDescriptor::Regular(inode) => inode.writable(),
            FileDescriptor::Abstract(inode) => inode.writable(),
        }
    }
    
    fn read(&self, buf: UserBuffer) -> usize {
        match self {
            FileDescriptor::Regular(inode) => inode.read(buf),
            FileDescriptor::Abstract(inode) => inode.read(buf),
        }
    }
    
    fn write(&self, buf: UserBuffer) -> usize {
        match self {
            FileDescriptor::Regular(inode) => inode.write(buf),
            FileDescriptor::Abstract(inode) => inode.write(buf),
        }
    }
}

unsafe impl Sync for FileDescriptor {}

unsafe impl Send for FileDescriptor {}
