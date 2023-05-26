use super::id::TaskUserRes;
use super::{kstack_alloc, KernelStack, ProcessControlBlock, TaskContext};
use crate::trap::TrapContext;
use crate::{
    mm::PhysPageNum,
    sync::{UPIntrFreeCell, UPIntrRefMut},
};
use alloc::sync::{Arc, Weak};

// 线程控制块
pub struct TaskControlBlock {
    // immutable
    pub process: Weak<ProcessControlBlock>, // 所属进程的弱引用
    pub kstack: KernelStack,                // 自身的内核栈
    // mutable
    pub inner: UPIntrFreeCell<TaskControlBlockInner>,
}

impl TaskControlBlock {
    pub fn inner_exclusive_access(&self) -> UPIntrRefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }

    pub fn get_user_token(&self) -> usize {
        let process = self.process.upgrade().unwrap();
        let inner = process.inner_exclusive_access();
        inner.memory_set.token()
    }
}

pub struct TaskControlBlockInner {
    pub res: Option<TaskUserRes>, // 线程资源集合
    pub trap_cx_ppn: PhysPageNum,
    pub task_cx: TaskContext,    // Trap 上下文
    pub task_status: TaskStatus, // 线程状态
    pub exit_code: Option<i32>,  // 线程退出码
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }

    #[allow(unused)]
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
}

impl TaskControlBlock {
    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        alloc_user_res: bool,
    ) -> Self {
        let res = TaskUserRes::new(Arc::clone(&process), ustack_base, alloc_user_res);
        let trap_cx_ppn = res.trap_cx_ppn();
        let kstack = kstack_alloc();
        let kstack_top = kstack.get_top();
        Self {
            process: Arc::downgrade(&process),
            kstack,
            inner: unsafe {
                UPIntrFreeCell::new(TaskControlBlockInner {
                    res: Some(res),
                    trap_cx_ppn,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    task_status: TaskStatus::Ready,
                    exit_code: None,
                })
            },
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    Blocked,
}
