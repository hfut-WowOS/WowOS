use super::{ProcessControlBlock, TaskControlBlock, TaskStatus};
use crate::sync::UPIntrFreeCell;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use lazy_static::*;

pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.ready_queue.pop_front()
    }
}

lazy_static! {
    pub static ref TASK_MANAGER: UPIntrFreeCell<TaskManager> =
        unsafe { UPIntrFreeCell::new(TaskManager::new()) };
    pub static ref PID2PCB: UPIntrFreeCell<BTreeMap<usize, Arc<ProcessControlBlock>>> =
        unsafe { UPIntrFreeCell::new(BTreeMap::new()) };
}

/// 将线程加入就绪队列
pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

/// 从事件的等待队列中取出线程，并调用唤醒线程
pub fn wakeup_task(task: Arc<TaskControlBlock>) {
    let mut task_inner = task.inner_exclusive_access();
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    add_task(task);
}

/// 从就绪队列中选出一个线程分配 CPU 资源
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}

/// 根据 PID 查询进程控制块
pub fn pid2process(pid: usize) -> Option<Arc<ProcessControlBlock>> {
    let map = PID2PCB.exclusive_access();
    map.get(&pid).map(Arc::clone)
}

/// 增加一对 PID-进程控制块映射
pub fn insert_into_pid2process(pid: usize, process: Arc<ProcessControlBlock>) {
    PID2PCB.exclusive_access().insert(pid, process);
}

/// 删除一对 PID-进程控制块映射
pub fn remove_from_pid2process(pid: usize) {
    let mut map = PID2PCB.exclusive_access();
    if map.remove(&pid).is_none() {
        panic!("cannot find pid {} in pid2task!", pid);
    }
}
