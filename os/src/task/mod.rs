mod context;
mod id;
mod manager;
mod process;
mod processor;
mod signal;
mod switch;
#[allow(clippy::module_inception)]
mod task;
use crate::fs::ROOT_INODE;

use self::id::TaskUserRes;
use crate::fs::{open_file, OpenFlags};
use crate::sbi::shutdown;
use alloc::{sync::Arc, vec::Vec};
use lazy_static::*;
use manager::fetch_task;
use process::ProcessControlBlock;
use switch::__switch;

pub use process::WorkPath;
pub use context::TaskContext;
pub use id::{kstack_alloc, pid_alloc, KernelStack, PidHandle, IDLE_PID};
pub use manager::{add_task, pid2process, remove_from_pid2process, wakeup_task};
pub use processor::*;
pub use signal::SignalFlags;
pub use task::{TaskControlBlock, TaskStatus};

pub fn suspend_current_and_run_next() {
    // There must be an application running.
    let task = take_current_task().unwrap();

    // ---- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    // ---- release current TCB

    // push back to ready queue.
    add_task(task);
    // jump to scheduling cycle
    schedule(task_cx_ptr);
}

/// This function must be followed by a schedule
pub fn block_current_task() -> *mut TaskContext {
    let task = take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    task_inner.task_status = TaskStatus::Blocked;
    &mut task_inner.task_cx as *mut TaskContext
}

pub fn block_current_and_run_next() {
    let task_cx_ptr = block_current_task();
    schedule(task_cx_ptr);
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next(exit_code: i32) {
    let task = take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    let process = task.process.upgrade().unwrap();
    let tid = task_inner.res.as_ref().unwrap().tid;
    // record exit code
    task_inner.exit_code = Some(exit_code);
    task_inner.res = None;
    // here we do not remove the thread since we are still using the kstack
    // it will be deallocated when sys_waittid is called
    drop(task_inner);
    drop(task);
    // however, if this is the main thread of current process
    // the process should terminate at once
    if tid == 0 {
        let pid = process.getpid();
        if pid == IDLE_PID {
            println!(
                "[kernel] Idle process exit with exit_code {} ...",
                exit_code
            );
            if exit_code != 0 {
                //crate::sbi::shutdown(255); //255 == -1 for err hint
                shutdown(true);
            } else {
                //crate::sbi::shutdown(0); //0 for success hint
                shutdown(false);
            }
        }
        remove_from_pid2process(pid);
        let mut process_inner = process.inner_exclusive_access();
        // mark this process as a zombie process
        process_inner.is_zombie = true;
        // record exit code of main process
        process_inner.exit_code = exit_code;

        {
            // move all child processes under init process
            let mut initproc_inner = INITPROC.inner_exclusive_access();
            for child in process_inner.children.iter() {
                child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
                initproc_inner.children.push(child.clone());
            }
        }

        // deallocate user res (including tid/trap_cx/ustack) of all threads
        // it has to be done before we dealloc the whole memory_set
        // otherwise they will be deallocated twice
        let mut recycle_res = Vec::<TaskUserRes>::new();
        for task in process_inner.tasks.iter().filter(|t| t.is_some()) {
            let task = task.as_ref().unwrap();
            let mut task_inner = task.inner_exclusive_access();
            if let Some(res) = task_inner.res.take() {
                recycle_res.push(res);
            }
        }
        // dealloc_tid and dealloc_user_res require access to PCB inner, so we
        // need to collect those user res first, then release process_inner
        // for now to avoid deadlock/double borrow problem.
        drop(process_inner);
        recycle_res.clear();

        let mut process_inner = process.inner_exclusive_access();
        process_inner.children.clear();
        // deallocate other data in user space i.e. program code/data section
        process_inner.memory_set.recycle_data_pages();
        // drop file descriptors
        process_inner.fd_table.clear();
    }
    drop(process);
    // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
}

lazy_static! {
    // pub static ref INITPROC: Arc<ProcessControlBlock> = {
    //     let inode = open_file(ROOT_INODE.clone(), "initproc", OpenFlags::RDONLY).unwrap();
    //     let v = inode.read_all();
    //     ProcessControlBlock::new(v.as_slice())
    // };
    pub static ref INITPROC: Arc<ProcessControlBlock> = {
        extern "C" {
            fn _num_app();
        }
        let num_app_ptr = _num_app as usize as *const usize;
        let num_app = unsafe { num_app_ptr.read_volatile() };
        let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };

       ProcessControlBlock::new( unsafe{
            core::slice::from_raw_parts(
                app_start[0] as *const u8,
                app_start[1] - app_start[0]
            ) }
        )
    };
        // 从文件系统中读取 initproc 程序的 elf 数据加载
        // let inode = open_file("initproc", OpenFlags::O_RDONLY).unwrap();
        // let v = inode.read_all();
        // TaskControlBlock::new(v.as_slice())
}

pub fn add_initproc() {
    let _initproc = INITPROC.clone();
}

pub fn check_signals_of_current() -> Option<(i32, &'static str)> {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    process_inner.signals.check_error()
}

pub fn current_add_signal(signal: SignalFlags) {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.signals |= signal;
}

// Use to clone thread
bitflags! {
    pub struct CloneFlag: usize{
        const CLONE_SIGHLD = 17;
        const CSIGNAL	    =	0x000000ff;	
        const CLONE_VM	    =   0x00000100;
        const CLONE_FS      =	0x00000200;	
        const CLONE_FILES   =	0x00000400;
        const CLONE_SIGHAND =	0x00000800;	
        const CLONE_PIDFD	=   0x00001000;	
        const CLONE_PTRACE	=   0x00002000;
        const CLONE_VFORK	=   0x00004000;
        const CLONE_PARENT	=   0x00008000;
        const CLONE_THREAD	=   0x00010000;
        const CLONE_NEWNS	=   0x00020000;
        const CLONE_SYSVSEM =	0x00040000;
        const CLONE_SETTLS	=   0x00080000;	
        const CLONE_PARENT_SETTID	=   0x00100000;
        const CLONE_CHILD_CLEARTID	=   0x00200000;
        const CLONE_DETACHED		=   0x00400000;
        const CLONE_UNTRACED	    =	0x00800000;	
        const CLONE_CHILD_SETTID	=   0x01000000;
        const CLONE_NEWCGROUP	    =	0x02000000;	
        const CLONE_NEWUTS	=	0x04000000;	
        const CLONE_NEWIPC	=	0x08000000;
        const CLONE_NEWUSER	=	0x10000000;	
        const CLONE_NEWPID	=	0x20000000;	
        const CLONE_NEWNET	=	0x40000000;	
        const CLONE_IO		=   0x80000000;
    }
}
