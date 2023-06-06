use crate::fs::{make_pipe, open, File, Kstat, OpenFlags, Statfs, Dirent, chdir, MNT_TABLE};
use crate::mm::{translated_byte_buffer, translated_refmut, translated_str, UserBuffer};
use crate::task::{current_process, current_user_token};
use alloc::sync::Arc;

use super::*;

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
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
        let file = file.clone();
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

// rcore 指导书中的open
#[allow(unused)]
pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let process = current_process();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open("/", path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = process.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

const AT_FDCWD: isize = -100;
const FD_LIMIT: usize = 128;
pub fn sys_openat(dirfd: isize, path: *const u8, flags: u32, mode: u32) -> isize {
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();

    let path = translated_str(token, path);

    // todo
    _ = mode;
    let oflags = OpenFlags::from_bits(flags).expect("[DEBUG] sys_openat: unsupported open flag!");
    // info!(
    //     "[DEBUG] enter sys_openat: dirfd:{}, path:{}, flags:{:?}, mode:{:o}",
    //     dirfd, path, oflags, mode
    // );
    if dirfd == AT_FDCWD {
        // 如果是当前工作目录
        if let Some(inode) = open(&inner.work_path, path.as_str(), oflags) {
            let fd = inner.alloc_fd();
            if fd == FD_LIMIT {
                return -EMFILE;
            }
            inner.fd_table[fd] = Some(inode);
            // info!("[DEBUG] sys_openat return new fd:{}", fd);
            fd as isize
        } else {
            // println!("[WARNING] sys_openat: can't open file:{}, return -1", path);
            -1
        }
    } else {
        let dirfd = dirfd as usize;
        // dirfd 不合法
        if dirfd >= inner.fd_table.len() && dirfd > FD_LIMIT {
            return -1;
        }
        if let Some(file) = &inner.fd_table[dirfd] {
            if let Some(tar_f) = open(file.get_name(), path.as_str(), oflags) {
                let fd = inner.alloc_fd();
                if fd == FD_LIMIT {
                    return -EMFILE;
                }
                inner.fd_table[fd] = Some(tar_f);
                // info!("[DEBUG] sys_openat return new fd:{}", fd);
                fd as isize
            } else {
                println!("[WARNING] sys_openat: can't open file:{}, return -1", path);
                -1
            }
        } else {
            // dirfd 对应条目为 None
            println!("[WARNING] sys_read: fd {} is none, return -1", dirfd);
            -1
        }
    }
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

pub fn sys_pipe(pipe: *mut usize) -> isize {
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(pipe_read);
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(pipe_write);
    *translated_refmut(token, pipe) = read_fd;
    *translated_refmut(token, unsafe { pipe.add(1) }) = write_fd;
    0
}

/*
## 这两个函数有以下区别：

- 功能不同：

    - sys_dup函数用于复制一个文件描述符，创建一个新的文件描述符，该文件描述符与原文件描述符指向相同的文件。
    - sys_dup3函数也用于复制一个文件描述符，但它可以指定新的文件描述符的数值，而不仅仅是分配一个连续的可用文件描述符。

- 参数不同：

    - sys_dup函数只接受一个参数fd，表示要复制的文件描述符。
    - sys_dup3函数接受两个参数old_fd和new_fd，分别表示要复制的旧文件描述符和要分配的新文件描述符。

- 错误处理不同：

    - sys_dup函数在文件描述符超出范围或原文件描述符对应的文件不存在时返回-1表示错误。
    - sys_dup3函数在旧文件描述符超出范围、新文件描述符超过限制或原文件描述符对应的文件不存在时返回-1表示错误。

- 复制行为不同：

    - sys_dup函数直接将原文件描述符对应的文件克隆一份，并将克隆后的文件放入新文件描述符的位置。
    - sys_dup3函数与sys_dup类似，但它可以指定新文件描述符的数值，不一定需要连续的可用文件描述符。
 */

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
    // 将旧文件描述符对应的文件克隆，并将克隆后的文件放入新文件描述符的位置。
    inner.fd_table[new_fd] = Some(Arc::clone(inner.fd_table[fd].as_ref().unwrap()));
    new_fd as isize
}

pub fn sys_dup3(old_fd: usize, new_fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    // 如果旧文件描述符超出了长度或者新文件描述符超过了限制，则返回-1表示错误。
    if old_fd >= inner.fd_table.len() || new_fd > FD_LIMIT {
        return -1;
    }
    // 检查旧文件描述符对应的文件是否存在。
    // 如果文件不存在，则返回-1表示错误。
    if inner.fd_table[old_fd].is_none() {
        return -1;
    }
    // 检查新文件描述符是否超出了进程文件描述符表的长度。
    if new_fd >= inner.fd_table.len() {
        for _ in inner.fd_table.len()..(new_fd + 1) {
            inner.fd_table.push(None);
        }
    }
    inner.fd_table[new_fd] = Some(inner.fd_table[old_fd].as_ref().unwrap().clone());
    new_fd as isize
}

/// sys_mkdirat函数用于在指定的目录下创建一个新的子目录。
/// dirfd: isize：表示目录文件描述符，可以是以下两个特殊值之一：
/// AT_FDCWD：表示当前工作目录。
/// 其他整数值：表示有效的文件描述符，指向一个已打开的目录。
/// path: *const u8：表示要创建的子目录的路径，以C风格的字符串形式表示。
/// mode: u32：表示创建目录的权限模式，目前该参数未使用。
pub fn sys_mkdirat(dirfd: isize, path: *const u8, mode: u32) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let path = translated_str(token, path);
    _ = mode;
    // 如果dirfd为AT_FDCWD，表示在当前工作目录下创建子目录：
    if dirfd == AT_FDCWD {
        // 调用open函数以读写和创建目录的方式打开指定路径的目录文件。
        if let Some(_) = open(
            &inner.work_path,
            path.as_str(),
            OpenFlags::O_DIRECTROY | OpenFlags::O_CREATE,
        ) {
            0
        } else {
            -1
        }
    } else {
        // 在指定的目录文件描述符所指向的目录下创建子目录
        let dirfd = dirfd as usize;
        // 检查dirfd是否超出范围或大于文件描述符的限制值
        if dirfd >= inner.fd_table.len() && dirfd > FD_LIMIT {
            return -1;
        }
        // 获取指定文件描述符对应的文件对象，并使用其路径和传入的path参数调用open函数以读写和创建目录的方式打开目录文件。
        if let Some(file) = &inner.fd_table[dirfd] {
            if let Some(_) = open(
                file.get_name(),
                path.as_str(),
                OpenFlags::O_DIRECTROY | OpenFlags::O_CREATE,
            ) {
                0
            } else {
                -1
            }
        } else {
            -1
        }
    }
}

/// 函数用于获取文件系统的信息
/// path: *const u8：表示文件系统的路径，目前该参数未使用。
/// buf: *const u8：表示用于存储统计信息的缓冲区的指针。
pub fn sys_statfs(path: *const u8, buf: *const u8) -> isize {
    let token = current_user_token();
    _ = path;
    let mut userbuf = UserBuffer::new(translated_byte_buffer(token, buf, size_of::<Statfs>()));
    // 将Statfs对象的字节表示写入userbuf
    userbuf.write(Statfs::new().as_bytes());
    0
}

use core::mem::size_of;

#[allow(unused)]
pub fn sys_newfstatat(
    dirfd: isize,
    pathname: *const u8,
    satabuf: *const usize,
    _flags: usize,
) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let path = translated_str(token, pathname);
    let buf_vec = translated_byte_buffer(token, satabuf as *const u8, size_of::<Kstat>());
    let mut userbuf = UserBuffer::new(buf_vec);
    let mut kstat = Kstat::new();

    if dirfd == AT_FDCWD {
        if let Some(inode) = open(&inner.work_path, path.as_str(), OpenFlags::O_RDONLY) {
            inode.get_fstat(&mut kstat);
            userbuf.write(kstat.as_bytes());
            // panic!();
            0
        } else {
            -ENOENT
        }
    } else {
        let dirfd = dirfd as usize;
        if dirfd >= inner.fd_table.len() && dirfd > FD_LIMIT {
            return -1;
        }
        if let Some(file) = &inner.fd_table[dirfd] {
            if let Some(inode) = open(file.get_name(), path.as_str(), OpenFlags::O_RDONLY) {
                inode.get_fstat(&mut kstat);
                userbuf.write(kstat.as_bytes());
                0
            } else {
                -1
            }
        } else {
            -ENOENT
        }
    }
}

// 用于表示文件定位标志的位标志（bitflags）
bitflags! {
    pub struct SeekFlags: usize {
        const SEEK_SET = 0;   // 参数 offset 即为新的读写位置
        const SEEK_CUR = 1;   // 以目前的读写位置往后增加 offset 个位移量
        const SEEK_END = 2;   // 将读写位置指向文件尾后再增加 offset 个位移量
    }
}

/// sys_lseek函数用于改变文件的读写位置。
/// fd: usize：表示文件描述符。
/// off_t: usize：表示偏移量。
/// whence: usize：表示定位标志，可以是SeekFlags中定义的常量之一。
pub fn sys_lseek(fd: usize, off_t: usize, whence: usize) -> isize {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    // 文件描述符不合法
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let flag = SeekFlags::from_bits(whence).unwrap();
        match flag {
            SeekFlags::SEEK_SET => {
                file.set_offset(off_t);
                off_t as isize
            }
            SeekFlags::SEEK_CUR => {
                let current_offset = file.get_offset();
                file.set_offset(off_t + current_offset);
                (off_t + current_offset) as isize
            }
            SeekFlags::SEEK_END => {
                let end = file.file_size();
                file.set_offset(end + off_t);
                (end + off_t) as isize
            }
            // flag wrong
            _ => panic!("sys_lseek: unsupported whence!"),
        }
    } else {
        // file not exists
        -3
    }
}

/// sys_getdents64是一个用于在Linux中检索目录条目的系统调用。
/// 它从由打开的文件描述符fd引用的目录中读取多个linux_dirent结构，并将它们放入由buf指向的缓冲区中。
/// 参数count指定了该缓冲区的大
/// 函数返回成功读取的目录项总字节数作为结果，或返回错误码表示操作失败。
pub fn sys_getdents64(fd: isize, buf: *mut u8, count: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let work_path = &inner.work_path;
    // 将缓冲区的指针buf翻译为内核地址空间
    let buf_vec = translated_byte_buffer(token, buf, count);
    // 用于对翻译后的缓冲区进行读写操作
    let mut userbuf = UserBuffer::new(buf_vec);
    // 创建一个Dirent对象dirent，用于存储读取到的目录项数据
    let mut dirent = Dirent::new();
    let dent_len = size_of::<Dirent>();
    let mut total_len: usize = 0;
    // 如果fd是AT_FDCWD，表示当前目录
    if fd == AT_FDCWD {
        // 如果成功打开当前目录（根目录）
        if let Some(file) = open("/", work_path.as_str(), OpenFlags::O_RDONLY) {
            loop {
                if total_len + dent_len > count {
                    break;
                }
                // 读取目录项数据到dirent
                // 如果成功读取到目录项数据（返回值大于0），将目录项数据写入userbuf的指定位置（total_len）
                if file.get_dirent(&mut dirent) > 0 {
                    userbuf.write_at(total_len, dirent.as_bytes());
                    total_len += dent_len;
                } else {
                    break;
                }
            }
            // 返回total_len作为结果，表示成功读取的目录项总字节数
            return total_len as isize;
        } else {
            return -1;
        }
    } else {
        if let Some(file) = &inner.fd_table[fd as usize] {
            loop {
                // 如果total_len + dent_len大于缓冲区长度len，表示缓冲区已满，退出循环
                if total_len + dent_len > count {
                    break;
                }
                // 调用文件的get_dirent方法读取目录项数据到dirent
                if file.get_dirent(&mut dirent) > 0 {
                    userbuf.write_at(total_len, dirent.as_bytes());
                    total_len += dent_len;
                } else {
                    break;
                }
            }
            return total_len as isize;
        } else {
            return -1;
        }
    }
}

/// 改变当前工作目录
/// 如果切换成功，则更新进程的内部状态以反映新的工作目录，并返回成功状态码0。
/// 如果切换失败，则返回错误状态码-1。
pub fn sys_chdir(path: *const u8) -> isize {
    let token = current_user_token();
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let path = translated_str(token, path);
    if let Some(new_cwd) = chdir(&inner.work_path, &path) {
        // 将进程的inner.work_path更新为新的工作目录。
        inner.work_path = new_cwd;
        0
    } else {
        -1
    }
}

pub fn sys_getcwd(buf: *mut u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();

    if buf as usize == 0 {
        unimplemented!();
    } else {
        let buf_vec = translated_byte_buffer(token, buf, len);
        let mut userbuf = UserBuffer::new(buf_vec);
        let cwd = inner.work_path.as_bytes();
        userbuf.write(cwd);
        userbuf.write_at(cwd.len(), &[0]); // 添加字符串末尾的\0
        return buf as isize;
    }
}

pub fn sys_mount(special: *const u8, dir: *const u8, fstype: *const u8, flags: usize, data: *const u8) -> isize {
    let token = current_user_token();
    let special = translated_str(token, special);
    let dir = translated_str(token, dir);
    let fstype = translated_str(token, fstype);
    _ = data;
    MNT_TABLE.lock().mount(special, dir, fstype, flags as u32)
}

pub fn sys_umount(p_special: *const u8, flags: usize) -> isize {
    let token = current_user_token();
    let special = translated_str(token, p_special);
    MNT_TABLE.lock().umount(special, flags as u32)
}

pub fn sys_unlinkat(fd: isize, path: *const u8, flags: u32) -> isize {
    let task = current_process();
    let token = current_user_token();
    let inner = task.inner_exclusive_access();
    // todo
    _ = flags;

    let path = translated_str(token, path);
    // println!("[DEBUG] enter sys_unlinkat: fd:{}, path:{}, flags:{}",fd,path,flags);
    if fd == AT_FDCWD {
        if let Some(file) = open(&inner.work_path, path.as_str(), OpenFlags::O_RDWR) {
            file.delete();
            0
        } else {
            -1
        }
    } else {
        unimplemented!();
    }
}

pub fn sys_fstat(fd: isize, buf: *mut u8) -> isize {
    let token = current_user_token();
    let process = current_process();
    let buf_vec = translated_byte_buffer(token, buf, size_of::<Kstat>());
    let inner = process.inner_exclusive_access();
    let mut userbuf = UserBuffer::new(buf_vec);
    let mut kstat = Kstat::new();
    let dirfd = fd as usize;
    if dirfd >= inner.fd_table.len() && dirfd > FD_LIMIT {
        return -1;
    }
    if let Some(file) = &inner.fd_table[dirfd] {
        file.get_fstat(&mut kstat);
        userbuf.write(kstat.as_bytes());
        0
    } else {
        -1
    }
}
// // to do
// #[allow(unused)]
// pub fn sys_linkat(
//     _oldfd: isize, 
//     _oldpath: *const u8, 
//     _newfd: isize, 
//     _newpath: *const u8, 
//     _flags: u32
// ) ->  isize {
//     0
// }