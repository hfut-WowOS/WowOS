use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}

/// 功能：为当前进程新增一把互斥锁。
/// 参数： blocking 为 true 表示互斥锁基于阻塞机制实现，
/// 否则表示互斥锁基于类似 yield 的方法实现。
/// 返回值：假设该操作必定成功，返回创建的锁的 ID 。
/// syscall ID: 1010
pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() as isize - 1
    }
}

/// 功能：当前线程尝试获取所属进程的一把互斥锁。
/// 参数： mutex_id 表示要获取的锁的 ID 。
/// 返回值： 0
/// syscall ID: 1011
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.lock();
    0
}

/// 功能：当前线程释放所属进程的一把互斥锁。
/// 参数： mutex_id 表示要释放的锁的 ID 。
/// 返回值： 0
/// syscall ID: 1012
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}

/// 功能：为当前进程新增一个信号量。
/// 参数：res_count 表示该信号量的初始资源可用数量，即 N ，为一个非负整数。
/// 返回值：假定该操作必定成功，返回创建的信号量的 ID 。
/// syscall ID : 1020
pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}

/// 功能：对当前进程内的指定信号量进行 V 操作。
/// 参数：sem_id 表示要进行 V 操作的信号量的 ID 。
/// 返回值：假定该操作必定成功，返回 0 。
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    sem.up();
    0
}

/// 功能：对当前进程内的指定信号量进行 P 操作。
/// 参数：sem_id 表示要进行 P 操作的信号量的 ID 。
/// 返回值：假定该操作必定成功，返回 0 。
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    sem.down();
    0
}

/// 功能：为当前进程新增一个条件变量。
/// 返回值：假定该操作必定成功，返回创建的条件变量的 ID 。
/// syscall ID : 1030
pub fn sys_condvar_create() -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}

/// 功能：对当前进程的指定条件变量进行 signal 操作，即
/// 唤醒一个在该条件变量上阻塞的线程（如果存在）。
/// 参数：condvar_id 表示要操作的条件变量的 ID 。
/// 返回值：假定该操作必定成功，返回 0 。
/// syscall ID : 1031
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}

/// 功能：对当前进程的指定条件变量进行 wait 操作，分为多个阶段：
/// 1. 释放当前线程持有的一把互斥锁；
/// 2. 阻塞当前线程并将其加入指定条件变量的阻塞队列；
/// 3. 直到当前线程被其他线程通过 signal 操作唤醒；
/// 4. 重新获取当前线程之前持有的锁。
/// 参数：mutex_id 表示当前线程持有的互斥锁的 ID ，而
/// condvar_id 表示要操作的条件变量的 ID 。
/// 返回值：假定该操作必定成功，返回 0 。
/// syscall ID : 1032
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait_with_mutex(mutex);
    0
}
