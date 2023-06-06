use crate::fs::*;
use crate::mm::{translated_byte_buffer, translated_refmut, translated_str, UserBuffer};
use crate::task::{current_process, current_user_token, WorkPath};
use alloc::string::ToString;
use fatfs::DIRENT_SZ;

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
// #[allow(unused)]
// pub fn sys_open(path: *const u8, flags: u32) -> isize {
//     let process = current_process();
//     let token = current_user_token();
//     let path = translated_str(token, path);
//     if let Some(inode) = open_file("/", path.as_str(), OpenFlags::from_bits(flags).unwrap(), FileType::Regular) {
//         let mut inner = process.inner_exclusive_access();
//         let fd = inner.alloc_fd();
//         inner.fd_table[fd] = Some(inode);
//         fd as isize
//     } else {
//         -1
//     }
// }

const FD_LIMIT: usize = 128;
pub fn sys_openat(fd: isize, path: *const u8, flags: u32) -> isize {
    let process = current_process();
    let token = current_user_token();
    let path = translated_str(token, path);
    
    let flags = unsafe { OpenFlags::from_bits_unchecked(flags) };
    //获取要打开文件的inode
    match if WorkPath::is_abs_path(&path) {
        open_file("/", &path, flags, FileType::Regular)
    } else if fd == AT_FD_CWD {
        let work_path = process.inner_exclusive_access().work_path.to_string();
        open_file(&work_path, &path, flags, FileType::Regular)
    } else {
        ////相对于fd的相对路径
        let inner = process.inner_exclusive_access();
        let fd_usize = fd as usize;
        if fd_usize >= inner.fd_table.len() {
            return -1;
        }
        //todo rcore tutorial使用的锁和spin::mutex冲突了..
        let res = inner.fd_table[fd_usize].clone();
        drop(inner);
        match res {
            Some(FileDescriptor::Regular(os_inode)) => {
                if flags.contains(OpenFlags::O_CREATE) {
                    os_inode.create(&path, FileType::Regular)
                } else {
                    os_inode.find(&path, flags)
                }
            }
            _ => {
                return -1;
            }
        }
    } {
        None => -1,
        Some(os_inode) => {
            //alloc fd and push into fd table
            let mut inner = process.inner_exclusive_access();
            let ret_fd = inner.alloc_fd();
            inner.fd_table[ret_fd] = Some(FileDescriptor::Regular(os_inode));
            assert!(inner.fd_table[ret_fd].is_some());
            ret_fd as isize
        }
    }
}

pub fn sys_close(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() || inner.fd_table[fd].is_none() {
        return -1;
    }
    // 把 fd 对应的值取走，变为 None
    inner.fd_table[fd].take();
    0
}

pub fn sys_pipe(pipe: *mut u32, flag: usize) -> isize {
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();
    _ = flag;
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(FileDescriptor::Abstract(pipe_read));
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(FileDescriptor::Abstract(pipe_write));
    *translated_refmut(token, pipe as *mut [u32; 2]) = [read_fd as u32, write_fd as u32];
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
    //判断文件描述符是否合法
    if fd >= inner.fd_table.len() || inner.fd_table[fd].is_none() {
        return -1;
    }
    //查找空闲的文件描述符
    let new_fd = inner.alloc_fd();
    //分配文件描述符
    inner.fd_table[new_fd] = Some(inner.fd_table[fd].as_ref().unwrap().clone());
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
/// 
pub const AT_FD_CWD: isize = -100;

pub fn sys_mkdir(dir_fd: isize, path: *const u8, mode: u32) -> isize {
    let token = current_user_token();
    let pcb = current_process();
    let path = translated_str(token, path);

    match if WorkPath::is_abs_path(&path) {
        open_file("/", &path, OpenFlags::O_CREATE, FileType::Dir)
    } else if dir_fd == AT_FD_CWD {
        let work_path = pcb.inner_exclusive_access().work_path.to_string();
        open_file(&work_path, &path, OpenFlags::O_CREATE, FileType::Dir)
    } else {
        let inner = pcb.inner_exclusive_access();
        let fd_usize = dir_fd as usize;
        if fd_usize >= inner.fd_table.len() {
            return -1;
        }

        if let Some(FileDescriptor::Regular(os_inode)) = inner.fd_table[fd_usize].clone() {
            if !os_inode.is_dir() {
                return -1;
            }
            os_inode.create(&path, FileType::Regular)
        } else {
            return -1;
        }
    } {
        None => -1,
        Some(_) => 0,
    }
}

/// 函数用于获取文件系统的信息
/// path: *const u8：表示文件系统的路径，目前该参数未使用。
/// buf: *const u8：表示用于存储统计信息的缓冲区的指针。
pub fn sys_fstat(fd: isize, kstat: *const u8) -> isize {
    let size = core::mem::size_of::<Kstat>();
    let token = current_user_token();
    let mut user_buf = UserBuffer::new(translated_byte_buffer(token, kstat, size));
    let pcb = current_process();
    let mut kstat = Kstat::default();
    let os_inode = if fd == AT_FD_CWD {
        let work_path = pcb.inner_exclusive_access().work_path.to_string();
        match open_file("/", &work_path, OpenFlags::O_RDONLY, FileType::Regular) {
            None => return -1,
            Some(os_inode) => os_inode,
        }
    } else {
        let fd_usize = fd as usize;
        let inner = pcb.inner_exclusive_access();
    
        if fd_usize >= inner.fd_table.len() {
            return -1;
        }
        match &inner.fd_table[fd_usize] {
            Some(FileDescriptor::Regular(os_inode)) => os_inode.clone(),
            _ => return -1,
        }
    };
    
    os_inode.get_fstat(&mut kstat);
    user_buf.write(kstat.as_bytes());
    0
}

use core::mem::size_of;

// #[allow(unused)]
// pub fn sys_newfstatat(
//     dirfd: isize,
//     pathname: *const u8,
//     satabuf: *const usize,
//     _flags: usize,
// ) -> isize {
//     let token = current_user_token();
//     let process = current_process();
//     let inner = process.inner_exclusive_access();
//     let path = translated_str(token, pathname);
//     let buf_vec = translated_byte_buffer(token, satabuf as *const u8, size_of::<Kstat>());
//     let mut userbuf = UserBuffer::new(buf_vec);
//     let mut kstat = Kstat::new();

//     if dirfd == AT_FDCWD {
//         if let Some(inode) = open(&inner.work_path, path.as_str(), OpenFlags::O_RDONLY) {
//             inode.get_fstat(&mut kstat);
//             userbuf.write(kstat.as_bytes());
//             // panic!();
//             0
//         } else {
//             -ENOENT
//         }
//     } else {
//         let dirfd = dirfd as usize;
//         if dirfd >= inner.fd_table.len() && dirfd > FD_LIMIT {
//             return -1;
//         }
//         if let Some(file) = &inner.fd_table[dirfd] {
//             if let Some(inode) = open(file.get_name(), path.as_str(), OpenFlags::O_RDONLY) {
//                 inode.get_fstat(&mut kstat);
//                 userbuf.write(kstat.as_bytes());
//                 0
//             } else {
//                 -1
//             }
//         } else {
//             -ENOENT
//         }
//     }
// }

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
// pub fn sys_lseek(fd: usize, off_t: usize, whence: usize) -> isize {
//     let process = current_process();
//     let inner = process.inner_exclusive_access();
//     // 文件描述符不合法
//     if fd >= inner.fd_table.len() {
//         return -1;
//     }
//     if let Some(file) = &inner.fd_table[fd] {
//         let flag = SeekFlags::from_bits(whence).unwrap();
//         match flag {
//             SeekFlags::SEEK_SET => {
//                 file.set_offset(off_t);
//                 off_t as isize
//             }
//             SeekFlags::SEEK_CUR => {
//                 let current_offset = file.get_offset();
//                 file.set_offset(off_t + current_offset);
//                 (off_t + current_offset) as isize
//             }
//             SeekFlags::SEEK_END => {
//                 let end = file.file_size();
//                 file.set_offset(end + off_t);
//                 (end + off_t) as isize
//             }
//             // flag wrong
//             _ => panic!("sys_lseek: unsupported whence!"),
//         }
//     } else {
//         // file not exists
//         -3
//     }
// }

/// sys_getdents64是一个用于在Linux中检索目录条目的系统调用。
/// 它从由打开的文件描述符fd引用的目录中读取多个linux_dirent结构，并将它们放入由buf指向的缓冲区中。
/// 参数count指定了该缓冲区的大
/// 函数返回成功读取的目录项总字节数作为结果，或返回错误码表示操作失败。
pub fn sys_getdents64(fd: isize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let pcb = current_process();
    let mut user_buf = UserBuffer::new(translated_byte_buffer(token, buf, len));
    let dirent_size = core::mem::size_of::<Dirent>();
    let mut total_read = 0;

    //我为什么会喜欢这种写法(
    let dir_inode = if fd == AT_FD_CWD {
        let work_path = pcb.inner_exclusive_access().work_path.to_string();
        match open_file("/", &work_path, OpenFlags::O_RDONLY, FileType::Dir) {
            //当前目录下搜索不到文件
            None => return -1,
            Some(os_inode) => os_inode,
        }
    } else {
        let inner = pcb.inner_exclusive_access();
        let fd_usize = fd as usize;
        if fd_usize >= inner.fd_table.len() {
            return -1;
        }
        match &inner.fd_table[fd_usize] {
            Some(FileDescriptor::Regular(os_inode)) => os_inode.clone(),
            //文件未打开
            _ => return -1,
        }
    };

    let read_times = len / DIRENT_SZ;
    let mut dirent = Dirent::default();
    for _ in 0..read_times {
        if dir_inode.get_dirent(&mut dirent) > 0 {
            user_buf.write_at(total_read, dirent.as_bytes());
            total_read += dirent_size;
        }
    }
    
    if total_read == dir_inode.get_size() {
        0
    } else {
        dirent_size as isize
    }
}

/// 改变当前工作目录
/// 如果切换成功，则更新进程的内部状态以反映新的工作目录，并返回成功状态码0。
/// 如果切换失败，则返回错误状态码-1。
pub fn sys_chdir(path: *const u8) -> isize {
    let token = current_user_token();
    let pcb = current_process();
    let inner = pcb.inner_exclusive_access();
    let path = translated_str(token, path);

    //获取当前线程的work path
    let current_path = inner.work_path.to_string();
    drop(inner);
    //尝试切换目录
    let ret = ch_dir(&current_path, &path);
    //获取成功则更新工作目录
    if ret == 0 {
        let mut inner = pcb.inner_exclusive_access();
        inner.work_path.modify_path(&path);
    }
    ret
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
        let cwd = inner.work_path.to_string();
        userbuf.write(cwd.as_bytes());
        userbuf.write_at(cwd.len(), &[0]); // 添加字符串末尾的\0
        return buf as isize;
    }
}

pub fn sys_mount(
    special: *const u8,
    dir: *const u8,
    fstype: *const u8,
    flags: usize,
    data: *const u8,
) -> isize {
    let token = current_user_token();
    let special = translated_str(token, special);
    let dir = translated_str(token, dir);
    let fstype = translated_str(token, fstype);

    _ = data;

    MNT_TABLE.lock().mount(special,dir,fstype,flags as u32)

}

pub fn sys_umount(special: *const u8, flags: usize) -> isize {
    let token = current_user_token();
    let special = translated_str(token, special);
    MNT_TABLE.lock().umount(special, flags as u32)
}
pub fn sys_unlink(fd: isize, path: *const u8, flags: u32) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    let pcb = current_process();
    
    match if WorkPath::is_abs_path(&path) {
        open_file("/", &path, OpenFlags::O_RDWR, FileType::Regular)
    } else if fd == AT_FD_CWD {
        let work_path = pcb.inner_exclusive_access().work_path.to_string();
        open_file(&work_path, &path, OpenFlags::O_RDWR, FileType::Regular)
    } else {
        let fd_usize = fd as usize;
        let mut inner = pcb.inner_exclusive_access();
        
        if fd_usize >= inner.fd_table.len() {
            return -1;
        }
        
        match &inner.fd_table[fd_usize] {
            Some(FileDescriptor::Regular(os_inode)) => Some(os_inode.clone()),
            _ => return -1,
        }
    } {
        None => return -1,
        Some(os_inode) => {
            os_inode.delete();
        }
    }
    0
}
