use crate::fs::{open, OpenFlags};
use crate::mm::{translated_ref, translated_refmut, translated_str, align_up, translated_byte_buffer, UserBuffer};
use crate::task::*;
use crate::timer::get_time_ms;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_get_time() -> isize {
    get_time_ms() as isize
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().process.upgrade().unwrap().getpid() as isize
}

//获取父进程PID
pub fn sys_getppid() -> isize {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    let pcb_ref = inner.parent.as_ref();
    if let Some(pcb) = pcb_ref {
        pcb.upgrade().unwrap().getpid() as isize
    } else {
        1
    }
    
}

#[allow(unused)]
pub fn sys_fork() -> isize {
    let current_process = current_process();
    let new_process = current_process.fork();
    let new_pid = new_process.getpid();
    // modify trap context of new_task, because it returns immediately after switching
    let new_process_inner = new_process.inner_exclusive_access();
    let task = new_process_inner.tasks[0].as_ref().unwrap();
    let trap_cx = task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    new_pid as isize
}

/// ### 当前进程 fork/clone 出来一个子进程。
pub fn sys_clone(flags: usize, stack_ptr: usize, _ptid: usize, _tls: usize, _ctid: usize) -> isize {
    let pcb = current_process();
    let flags = unsafe { CloneFlag::from_bits_unchecked(flags) };
    let child_pcb = pcb.fork();
    let child_pid = child_pcb.getpid();
    
    if !flags.contains(CloneFlag::CLONE_SIGHLD) {
        return -1;
    }
    if flags.contains(CloneFlag::CLONE_CHILD_CLEARTID) {}
    if flags.contains(CloneFlag::CLONE_CHILD_SETTID) {}
    
    let child_inner = child_pcb.inner_exclusive_access();
    let child_task = child_inner.tasks[0].as_ref().unwrap();
    let child_trap_cx = child_task.inner.exclusive_access().get_trap_cx();
    
    //更改
    if stack_ptr != 0 {
        child_trap_cx.x[2] = stack_ptr;
    }
    child_trap_cx.x[10] = 0;
    child_pid as isize
}

/// ### 将当前进程的地址空间清空并加载一个特定的可执行文件，返回用户态后开始它的执行。
/// - 参数：
///     - `path` 给出了要加载的可执行文件的名字
///     - `args` 数组中的每个元素都是一个命令行参数字符串的起始地址，以地址为0表示参数尾
/// - 返回值：如果出错的话（如找不到名字相符的可执行文件）则返回 -1，否则返回参数个数 `argc`。
pub fn sys_execve(path: *const u8, mut args: *const usize) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    let mut args_vec: Vec<String> = Vec::new();
    loop {
        let arg_str_ptr = *translated_ref(token, args);
        if arg_str_ptr == 0 {
            break;
        }
        args_vec.push(translated_str(token, arg_str_ptr as *const u8));
        unsafe {
            args = args.add(1);
        }
    }

    //获取当前工作目录
    let work_path = current_process()
        .inner_exclusive_access()
        .work_path.clone();
    if let Some(app_inode) = open(
        &work_path,
        path.as_str(),
        OpenFlags::O_RDONLY,
    ) {
        let all_data = app_inode.read_all();
        let process = current_process();
        let argc = args_vec.len();
        process.exec(all_data.as_slice(), args_vec);
        // return argc because cx.x[10] will be covered with it later
        argc as isize
    } else {
        -1
    }
}

// 指导书中的exec
#[allow(unused)]
pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    let mut args_vec: Vec<String> = Vec::new();
    loop {
        let arg_str_ptr = *translated_ref(token, args);
        if arg_str_ptr == 0 {
            break;
        }
        args_vec.push(translated_str(token, arg_str_ptr as *const u8));
        unsafe {
            args = args.add(1);
        }
    }
    if let Some(app_inode) = open("/", path.as_str(), OpenFlags::O_RDONLY) {
        let all_data = app_inode.read_all();
        let process = current_process();
        let argc = args_vec.len();
        process.exec(all_data.as_slice(), args_vec);
        // return argc because cx.x[10] will be covered with it later
        argc as isize
    } else {
        -1
    }
}


/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
#[allow(unused)]
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let process = current_process();
    // find a child process

    let mut inner = process.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

pub fn sys_wait4(pid: isize, status: *mut i32, options: isize) -> isize {
    //参数options提供了一些另外的选项来控制waitpid()函数的行为。如果不想使用这些选项，则可以把这个参数设为0。
    if options != 0{
        panic!{"Extended option not support yet..."};
    }
    loop {
        let process = current_process();
        // find a child process
        //failed return-1
        let mut inner = process.inner_exclusive_access();
        if !inner
            .children
            .iter()
            .any(|p| pid == -1 || pid as usize == p.getpid())
        {
            return -1;
            // ---- release current PCB
        }
        let pair = inner.children.iter().enumerate().find(|(_, p)| {
            // ++++ temporarily access child PCB exclusively
            p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
            // ++++ release child PCB
        });
        if let Some((idx, _)) = pair {
            let child = inner.children.remove(idx);
            // confirm that child will be deallocated after being removed from children list
            assert_eq!(Arc::strong_count(&child), 1);
            let found_pid = child.getpid();
            // ++++ temporarily access child PCB exclusively
            let exit_code = child.inner_exclusive_access().exit_code;
            // ++++ release child PCB

            //mjqadd
            let sstatus = exit_code << 8;
            if (status as usize) != 0 {
                *translated_refmut(inner.memory_set.token(), status) = sstatus;
            }
            return found_pid as isize;
        } else {
            drop(inner);
            drop(process);
            suspend_current_and_run_next();
        }
    }
    // ---- release current PCB automatically
}

pub fn sys_kill(pid: usize, signal: u32) -> isize {
    if let Some(process) = pid2process(pid) {
        if let Some(flag) = SignalFlags::from_bits(signal) {
            process.inner_exclusive_access().signals |= flag;
            0
        } else {
            -1
        }
    } else {
        -1
    }
}

pub fn sys_brk(addr: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if addr == 0 {
        inner.heap_end.0 as isize
    } else if addr < inner.heap_base.0 {
        -1
    } else {
        inner.heap_end = addr.into();
        addr as isize
    }
}

pub fn sys_mmap(
    start: usize,
    len: usize,
    prot: usize,
    flags: usize,
    fd: usize,
    offset: usize,
) -> isize {
    let align_start = align_up(current_process().inner_exclusive_access().mmap_area_end.0);
    let align_len = align_up(len);
    current_process().inner_exclusive_access().mmap(
        align_start,
        align_len,
        prot,
        flags,
        fd,
        offset,
    );
    lazy_check(align_start);
    align_start as isize
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    let align_start = align_up(start);
    if current_process()
        .inner_exclusive_access()
        .munmap(align_start, len)
    {
        0
    } else {
        -1
    }
}

// pub fn sys_nanosleep(buf:*mut u8) -> isize {
//     //获取当前时间
//     let start = get_time_ms();
//     let token = current_user_token();
//     let sleep_time = translated_refmut(token, buf as *mut TimeVal);
//     let time = sleep_time.sec*1000 + sleep_time.usec / 1000;
//     loop{
//         let end = get_time_ms();
//         if end - start >= time {
//             break;
//         }
//     };
//     0
// }


pub fn sys_uname(buf:*const u8) -> isize{
    //取出正在执行的用户地址空间
    let token = current_user_token();
    let uname = Utsname::new();
    //以向量的形式返回一组可以在内存空间中直接访问的字节数组切片buf_vec
    let mut buf_vec = translated_byte_buffer(token, buf, core::mem::size_of::<Utsname>());
    //抽象缓冲区，使内核可以访问
    let mut usebuffer = UserBuffer::new(buf_vec);
    //将系统信息写入缓冲区usebuffer
    usebuffer.write(uname.as_bytes());
    0   
}

pub fn sys_linkat(
    _oldfd: isize, 
    _oldpath: *const u8, 
    _newfd: isize, 
    _newpath: *const u8, 
    _flags: u32
) ->  isize {
    0
}
