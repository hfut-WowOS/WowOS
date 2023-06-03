use alloc::string::ToString;
use fat32::DIRENT_SZ;
use crate::fs::*;
use crate::task::WorkPath;
use crate::mm::{translated_byte_buffer, translated_refmut, translated_str, UserBuffer};
use crate::task::{current_process, current_user_token};

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

pub fn sys_open(fd: isize, path: *const u8, flags: u32) -> isize {
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
            Some(FileDescriptor::OSInode(os_inode)) => {
                if flags.contains(OpenFlags::CREATE) {
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
            inner.fd_table[ret_fd] = Some(FileDescriptor::OSInode(os_inode));
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
    inner.fd_table[read_fd] = Some(FileDescriptor::Other(pipe_read));
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(FileDescriptor::Other(pipe_write));
    *translated_refmut(token, pipe as *mut [u32; 2]) = [read_fd as u32, write_fd as u32];
    0
}
/// ### 将进程中一个已经打开的文件描述符复制一份并分配到一个新的文件描述符中。
/// - 参数：fd 表示进程中一个已经打开的文件的文件描述符。
/// - 返回值：
///     - 能够访问已打开文件的新文件描述符。
///     - 如果出现了错误则返回 -1，可能的错误原因是：传入的 fd 并不对应一个合法的已打开文件。
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

///buf：用于保存当前工作目录的字符串。当 buf 设为 NULL，由系统来分配缓存区
pub fn sys_get_cwd(buf: *mut u8, len: usize) -> isize {

    let token = current_user_token();
    //进程控制块
    let pcb = current_process();
    let inner = pcb.inner_exclusive_access();
    //对内存抽象，以便内核访问
    let mut userbuf = UserBuffer::new(translated_byte_buffer(token, buf, len));
    //获取当前工作目录的长度
    let ret = userbuf.write(inner.work_path.to_string().as_bytes());
    if ret == 0 {
        0
    } else {
        buf as isize
    }
}

pub fn sys_chdir(path: *const u8) -> isize {
    let token = current_user_token();
    let pcb = current_process();
    let mut inner = pcb.inner_exclusive_access();
    let path = translated_str(token, path);

    //获取当前线程的work path
    let mut current_path = inner.work_path.to_string();
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

pub const AT_FD_CWD: isize = -100;

pub fn sys_mkdir(dir_fd: isize, path: *const u8, mode: u32) -> isize {
    let token = current_user_token();
    let pcb = current_process();
    let path = translated_str(token, path);

    match if WorkPath::is_abs_path(&path) {
        open_file("/", &path, OpenFlags::CREATE, FileType::Dir)
    } else if dir_fd == AT_FD_CWD {
        let work_path = pcb.inner_exclusive_access().work_path.to_string();
        open_file(&work_path, &path, OpenFlags::CREATE, FileType::Dir)
    } else {
        let mut inner = pcb.inner_exclusive_access();
        let fd_usize = dir_fd as usize;
        if fd_usize >= inner.fd_table.len() {
            return -1;
        }

        if let Some(FileDescriptor::OSInode(os_inode)) = inner.fd_table[fd_usize].clone() {
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

/// ### 将进程中一个已经打开的文件描述符复制一份并分配到一个指定的文件描述符中。
/// - 参数：
///     - old_fd 表示进程中一个已经打开的文件的文件描述符。
///     - new_fd 表示进程中一个指定的文件描述符中。
/// - 返回值：
///     - 能够访问已打开文件的新文件描述符。
///     - 如果出现了错误则返回 -1，可能的错误原因是：
///         - 传入的 old_fd 为空。
///         - 传入的 old_fd 不存在
///         - 传入的 new_fd 超出描述符数量限制 (典型值：128)
pub fn sys_dup3(old_fd: usize, new_fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if old_fd >= inner.fd_table.len()
        || inner.fd_table[old_fd].is_none()
        || old_fd == new_fd
        || new_fd >= 128
    {
        // TODO: 128 is FD_MAX
        return -1;
    }
    // alloc a specific new_fd
    inner.alloc_specific_fd(new_fd);
    inner.fd_table[new_fd] = Some(inner.fd_table[old_fd].as_ref().unwrap().clone());
    new_fd as isize
}

//默认len 是 dirent_size的倍数
pub fn sys_getdents64(fd: isize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let pcb = current_process();
    let mut user_buf = UserBuffer::new(translated_byte_buffer(token, buf, len));
    let dirent_size = core::mem::size_of::<dirent>();
    let mut total_read = 0;

    let dir_inode = if fd == AT_FD_CWD {
        let work_path = pcb.inner_exclusive_access().work_path.to_string();
        match open_file("/", &work_path, OpenFlags::RDONLY, FileType::Dir) {
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
            Some(FileDescriptor::OSInode(os_inode)) => os_inode.clone(),
            //文件未打开
            _ => return -1,
        }
    };

    let read_times = len / DIRENT_SZ;
    let mut dirent = dirent::default();
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

//to do
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

pub fn sys_fstat(fd: isize, kstat: *const u8) -> isize {
    let size = core::mem::size_of::<Kstat>();
    let token = current_user_token();
    let mut user_buf = UserBuffer::new(translated_byte_buffer(token, kstat, size));
    let pcb = current_process();
    let mut kstat = Kstat::default();
    let os_inode = if fd == AT_FD_CWD {
        let work_path = pcb.inner_exclusive_access().work_path.to_string();
        match open_file("/", &work_path, OpenFlags::RDONLY, FileType::Regular) {
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
            Some(FileDescriptor::OSInode(os_inode)) => os_inode.clone(),
            _ => return -1,
        }
    };
    
    os_inode.get_fstat(&mut kstat);
    user_buf.write(kstat.as_bytes());
    0
}

pub fn sys_unlink(fd: isize, path: *const u8, flags: u32) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    let pcb = current_process();
    
    match if WorkPath::is_abs_path(&path) {
        open_file("/", &path, OpenFlags::RDWR, FileType::Regular)
    } else if fd == AT_FD_CWD {
        let work_path = pcb.inner_exclusive_access().work_path.to_string();
        open_file(&work_path, &path, OpenFlags::RDWR, FileType::Regular)
    } else {
        let fd_usize = fd as usize;
        let mut inner = pcb.inner_exclusive_access();
        
        if fd_usize >= inner.fd_table.len() {
            return -1;
        }
        
        match &inner.fd_table[fd_usize] {
            Some(FileDescriptor::OSInode(os_inode)) => Some(os_inode.clone()),
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
