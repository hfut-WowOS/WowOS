use crate::fs::*;
use crate::mm::{translated_byte_buffer, translated_refmut, translated_str, UserBuffer};
use crate::task::{current_process, current_user_token};
use alloc::sync::Arc;
use core::clone;

use super::process;

pub fn sys_getcwd(buf: *mut u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();

    let mut userbuf = UserBuffer::new(translated_byte_buffer(token, buf, len));
    let inner = process.inner_exclusive_access();
    let ret = userbuf.write(inner.cwd.as_bytes());
    if ret == 0 {
        0
    } else {
        buf as isize
    }
}

pub fn sys_dup(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    let new_fd = inner.alloc_fd();
    inner.fd_table[new_fd] = Some(inner.fd_table[fd].as_ref().unwrap().clone());
    new_fd as isize
}

pub fn sys_dup3(oldfd: usize, newfd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if oldfd == newfd {
        return oldfd as isize;
    }
    if oldfd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[oldfd].is_none() {
        return -1;
    }
    while newfd > inner.fd_table.len() - 1 {
        inner.fd_table.push(None);
    }
    inner.fd_table[newfd] = Some(inner.fd_table[oldfd].as_ref().unwrap().clone());
    newfd as isize
}

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = match file {
            FileDescriptor::OSInode(f) => f.clone(),
            FileDescriptor::Other(f) => f.clone(),
        };
        if !file.writable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = match file {
            FileDescriptor::OSInode(f) => f.clone(),
            FileDescriptor::Other(f) => f.clone(),
        };
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(fd: isize, path: *const u8, flags: u32) -> isize {
    let process = current_process();
    let token = current_user_token();
    let path = translated_str(token, path);
    let flags = unsafe { OpenFlags::from_bits_unchecked(flags) };
    if let Some(inode) = open_file_at(fd, path.as_str(), flags) {
        let mut inner = process.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(FileDescriptor::OSInode(inode));
        fd as isize
    } else {
        -1
    }
}

pub fn sys_openat(fd: isize, path: *const u8, flags: u32, mode: u32) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file_at(fd, path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let process = current_process();
        let mut inner = process.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(FileDescriptor::OSInode(inode));
        fd as isize
    } else {
        -1
    }
}

pub fn sys_mkdirat(dirfd: isize, path: *const u8, mode: u32) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    mkdir_at(dirfd, path.as_str())
}

pub fn sys_chdir(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    inner.cwd = path;
    0
}

pub fn sys_close(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

pub fn sys_fstat(fd: usize, buf: *const u8) -> isize {
    let token = current_user_token();
    let mut buf = UserBuffer::new(translated_byte_buffer(
        token,
        buf,
        core::mem::size_of::<stat>(),
    ));
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        match file {
            FileDescriptor::OSInode(inode) => {
                if let Some(stat) = inode.stat() {
                    buf.read(stat.as_bytes());
                    return 0;
                } else {
                    return -1;
                }
            }
            _ => return -1,
        }
    } else {
        return -1;
    }
}

pub fn sys_pipe2(pipe: *mut usize) -> isize {
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(FileDescriptor::Other(pipe_read));
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(FileDescriptor::Other(pipe_write));
    *translated_refmut(token, pipe) = read_fd;
    *translated_refmut(token, unsafe { pipe.add(1) }) = write_fd;
    0
}

pub fn sys_unlinkat(dirfd: isize, path: *const u8, flags: u32) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    unlinkat(dirfd, path.as_str())
}

// to do
pub fn sys_mount(
    _special: *const u8,
    _dir: *const u8,
    _fstype: *const u8,
    _flags: usize,
    _data: *const u8,
) -> isize {
    0
}

pub fn sys_umount(_special: *const u8, _flags: usize) -> isize {
    0
}

pub fn sys_getdents64(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let mut buf = UserBuffer::new(translated_byte_buffer(token, buf, len));
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        match file {
            FileDescriptor::OSInode(inode) => {
                if let Some(dirent) = inode.getdents64() {
                    return buf.read(dirent.as_bytes()) as isize;
                } else {
                    return -1;
                }
            }
            _ => return -1,
        }
    } else {
        return -1;
    }
}
