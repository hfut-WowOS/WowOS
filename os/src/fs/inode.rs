use super::info::*;
use super::{File, FileDescriptor};
use crate::drivers::BLOCK_DEVICE;
use crate::mm::UserBuffer;
use crate::sync::UPIntrFreeCell;
use crate::task::current_process;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;
use fat32::*;
use lazy_static::*;

// OSInode 表示进程中一个被打开的常规文件或目录
pub struct OSInode {
    // 表明该文件是否允许通过 sys_read/write 进行读写
    readable: bool,
    writable: bool,
    inner: UPIntrFreeCell<OSInodeInner>,
}

pub struct OSInodeInner {
    offset: usize,     // 偏移量
    inode: Arc<VFile>, // 文件系统中的inode
}

impl OSInode {
    pub fn new(readable: bool, writable: bool, inode: Arc<VFile>) -> Self {
        Self {
            readable,
            writable,
            inner: unsafe { UPIntrFreeCell::new(OSInodeInner { offset: 0, inode }) },
        }
    }

    pub fn seek(&self, off: usize) {
        self.inner.exclusive_access().offset = off;
    }

    pub fn delete(&self) -> isize {
        self.inner.exclusive_access().inode.remove();
        0
    }

    pub fn getdents64(&self) -> Option<dirent> {
        let offset = self.inner.exclusive_access().offset;
        if let Some((name, dinfo, doff, _)) =
            self.inner.exclusive_access().inode.dirent_info(offset)
        {
            Some(dirent::new(name, dinfo as u64, doff as i64, DT_DIR))
        } else {
            return None;
        }
    }

    /// 从文件中读出信息放入缓冲区中
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer = [0u8; 512];
        let mut v: Vec<u8> = Vec::new();
        loop {
            // 分块读取文件内容
            let len = inner.inode.read_at(inner.offset, &mut buffer);
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }

    pub fn write_all(&self, str_vec: &Vec<u8>) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut remain = str_vec.len();
        let mut base = 0;
        loop {
            let len = remain.min(512);
            inner
                .inode
                .write_at(inner.offset, &str_vec.as_slice()[base..base + len]);
            inner.offset += len;
            base += len;
            remain -= len;
            if remain == 0 {
                break;
            }
        }
        return base;
    }

    pub fn get_size(&self) -> usize {
        let inner = self.inner.exclusive_access();
        let (size, _, mt_me, _, _) = inner.inode.stat();
        return size as usize;
    }

    pub fn stat(&self) -> Option<stat> {
        let inner = self.inner.exclusive_access();
        let (size, atime, mtime, ctime, _) = inner.inode.stat();
        if let Some((_, _, dinfo, _)) = inner.inode.dirent_info(inner.offset) {
            Some(stat::new(
                0o664,
                dinfo as u64,
                3,
                1,
                size.max(inner.offset as i64),
                atime,
                mtime,
                ctime,
            ))
        } else {
            Some(stat::new(
                0o664,
                0,
                3,
                1,
                size.max(inner.offset as i64),
                atime,
                mtime,
                ctime,
            ))
        }
    }
}

lazy_static! {
    pub static ref ROOT_INODE: Arc<VFile> = {
        let fat32_manager = FAT32Manager::open(BLOCK_DEVICE.clone());
        let manager_reader = fat32_manager.read();
        Arc::new(manager_reader.get_root_vfile(&fat32_manager))
    };
}

pub fn list_apps() {
    println!("/**** APPS ****");
    for app in ROOT_INODE.ls().unwrap() {
        println!("{}", app.0);
    }
    println!("**************/")
}

bitflags! {
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RDWR = 1 << 1;
        const CREATE = 1 << 9;
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags {
    /// Do not check validity for simplicity
    /// Return (readable, writable)
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

pub fn open_file(dir: Arc<VFile>, name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();
    if flags.contains(OpenFlags::CREATE) {
        if let Some(inode) = dir.find_vfile_byname(name) {
            // clear size
            inode.clear();
            Some(Arc::new(OSInode::new(readable, writable, Arc::new(inode))))
        } else {
            // create file
            dir.create(name, ATTRIBUTE_ARCHIVE)
                .map(|inode| Arc::new(OSInode::new(readable, writable, inode)))
        }
    } else {
        dir.find_vfile_byname(name).map(|inode| {
            if flags.contains(OpenFlags::TRUNC) {
                inode.clear();
            }
            Arc::new(OSInode::new(readable, writable, Arc::new(inode)))
        })
    }
}

pub fn open_file_at(fd: isize, name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    if let Some(dir) = get_dir(fd, name) {
        let mut paths: Vec<&str> = name.split("/").collect();
        let name = paths.pop().unwrap();
        open_file(dir, name, flags)
    } else {
        None
    }
}

pub fn mkdir_at(fd: isize, name: &str) -> isize {
    if let Some(dir) = get_dir(fd, name) {
        let mut paths: Vec<&str> = name.split("/").collect();
        let name = paths.pop().unwrap();
        dir.create(name, ATTRIBUTE_DIRECTORY);
        0
    } else {
        return -1;
    }
}

pub fn unlinkat(fd: isize, name: &str) -> isize {
    if let Some(inode) = open_file_at(fd, name, OpenFlags::RDONLY) {
        inode.delete()
    } else {
        -1
    }
}

// 找到打开文件的父目录
fn get_dir(fd: isize, name: &str) -> Option<Arc<VFile>> {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let mut paths: Vec<&str> = name.split("/").collect();
    paths.pop().unwrap();

    let dir: Arc<VFile>;
    if paths.len() > 0 && paths[0] == "" {
        if let Some(tmp) = ROOT_INODE.find_vfile_bypath(paths.to_vec()) {
            dir = tmp;
        } else {
            return None;
        }
    } else if fd == -100 {
        // 相对于当前工作目录
        let mut cwd = inner.cwd.split("/").collect::<Vec<_>>();

        if paths.len() > 0 && paths[0] == ".." {
            cwd.pop();
        }
        cwd.append(&mut paths);

        if let Some(tmp) = ROOT_INODE.find_vfile_bypath(cwd.to_vec()) {
            dir = tmp;
        } else {
            return None;
        }
    } else {
        // 相对于 fd 的工作目录
        if let Some(cwd) = inner.fd_table[fd as usize].clone() {
            match cwd {
                FileDescriptor::OSInode(osinode) => {
                    if let Some(tmp) = osinode
                        .inner
                        .exclusive_access()
                        .inode
                        .find_vfile_bypath(paths.to_vec())
                    {
                        dir = tmp;
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        } else {
            return None;
        }
    }
    Some(dir)
}

// 第一个参数fd是常量AT_FDCWD时，则其后的第二个参数路径名是以当前工作目录为基址的；否则以fd指定的目录文件描述符为基址。
pub fn openat(fd: isize, name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    if let Some(dir) = get_dir(fd, name) {
        let mut paths: Vec<&str> = name.split("/").collect();
        let name = paths.pop().unwrap();
        open_file(dir, name, flags)
    } else {
        None
    }
}

impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }
    fn writable(&self) -> bool {
        self.writable
    }
    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = inner.inode.read_at(inner.offset, *slice);
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        total_read_size
    }
    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = inner.inode.write_at(inner.offset, *slice);
            assert_eq!(write_size, slice.len());
            inner.offset += write_size;
            total_write_size += write_size;
        }
        total_write_size
    }
}
