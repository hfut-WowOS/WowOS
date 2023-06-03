use super::id::RecycleAllocator;
use super::manager::insert_into_pid2process;
use super::TaskControlBlock;
use super::{add_task, SignalFlags};
use super::{pid_alloc, PidHandle};
use crate::config::MEMORY_MAP_BASE;
use alloc::string::{String, ToString};
use crate::fs::{FileDescriptor, Stdin, Stdout};
use crate::mm::{
    translated_refmut, MapPermission, MemoryMapArea, MemorySet, VirtAddr, VirtPageNum, KERNEL_SPACE,
};
use crate::sync::{Condvar, Mutex, Semaphore, UPIntrFreeCell, UPIntrRefMut};
use crate::trap::{trap_handler, TrapContext};
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;

// 进程控制块
pub struct ProcessControlBlock {
    // immutable
    pub pid: PidHandle,
    // mutable
    inner: UPIntrFreeCell<ProcessControlBlockInner>,
}

pub struct ProcessControlBlockInner {
    pub is_zombie: bool,
    pub memory_set: MemorySet,
    pub parent: Option<Weak<ProcessControlBlock>>,
    pub children: Vec<Arc<ProcessControlBlock>>,
    pub exit_code: i32,
    pub fd_table: Vec<Option<FileDescriptor>>,
    pub signals: SignalFlags,
    // 设置一个向量保存进程下所有线程的任务控制块
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    // 每个进程控制块中都有一个给进程内的线程分配资源的通用分配器
    pub task_res_allocator: RecycleAllocator,

    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    // 信号量
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    // 条件变量
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
    pub work_path: WorkPath,
    // pub cwd: String,
    // user_heap
    pub heap_base: VirtAddr,
    pub heap_end: VirtAddr,
    pub mmap_area_base: VirtAddr,
    pub mmap_area_end: VirtAddr,
}

impl ProcessControlBlockInner {
    #[allow(unused)]
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }

    /// 查找空闲文件描述符下标
    /// 从文件描述符表中 **由低到高** 查找空位，返回向量下标，没有空位则在最后插入一个空位
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }

    // alloc a specific new_fd
    pub fn alloc_specific_fd(&mut self, new_fd: usize) -> usize {
        for _ in self.fd_table.len()..=new_fd {
            self.fd_table.push(None);
        }
        new_fd
    }

    pub fn alloc_tid(&mut self) -> usize {
        self.task_res_allocator.alloc()
    }

    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid)
    }

    pub fn thread_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        self.tasks[tid].as_ref().unwrap().clone()
    }

    pub fn mmap(
        &mut self,
        start: usize,
        len: usize,
        prot: usize,
        flags: usize,
        fd: usize,
        offset: usize,
    ) {
        let start_va = start.into();
        let end_va = (start + len).into();
        // 测例prot定义与MapPermission正好差一位
        let map_perm = MapPermission::from_bits((prot << 1) as u8).unwrap() | MapPermission::U;

        self.memory_set.insert_mmap_area(MemoryMapArea::new(
            start_va, end_va, map_perm, fd, offset, flags,
        ));
        self.mmap_area_end = end_va;
    }

    pub fn munmap(&mut self, start: usize, len: usize) -> bool {
        let start_vpn = VirtPageNum::from(VirtAddr::from(start));
        self.memory_set.remove_mmap_area(start_vpn)
    }
}

impl ProcessControlBlock {
    pub fn inner_exclusive_access(&self) -> UPIntrRefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }
    // 只有init proc调用,其他的线程从fork产生
    pub fn new(elf_data: &[u8]) -> Arc<Self> {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, uheap_base, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        // allocate a pid
        let pid_handle = pid_alloc();
        let process = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPIntrFreeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(FileDescriptor::Other(Arc::new(Stdin))),
                        // 1 -> stdout
                        Some(FileDescriptor::Other(Arc::new(Stdout))),
                        // 2 -> stderr
                        Some(FileDescriptor::Other(Arc::new(Stdout))),
                    ],
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                    //cwd: String::from("/"),
                    work_path: WorkPath::new(),
                    heap_base: uheap_base.into(),
                    heap_end: uheap_base.into(),
                    mmap_area_base: MEMORY_MAP_BASE.into(),
                    mmap_area_end: MEMORY_MAP_BASE.into(),
                })
            },
        });
        // create a main thread, we should allocate ustack and trap_cx here
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&process),
            ustack_base,
            true,
        ));
        // prepare trap_cx of main thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        let ustack_top = task_inner.res.as_ref().unwrap().ustack_top();
        let kstack_top = task.kstack.get_top();
        drop(task_inner);
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            ustack_top,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );
        // add main thread to the process
        let mut process_inner = process.inner_exclusive_access();
        process_inner.tasks.push(Some(Arc::clone(&task)));
        drop(process_inner);
        insert_into_pid2process(process.getpid(), Arc::clone(&process));
        // add main thread to scheduler
        add_task(task);
        process
    }

    /// Only support processes with a single thread.
    pub fn exec(self: &Arc<Self>, elf_data: &[u8], args: Vec<String>) {
        assert_eq!(self.inner_exclusive_access().thread_count(), 1);
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, uheap_base, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        let new_token = memory_set.token();
        // substitute memory_set
        self.inner_exclusive_access().memory_set = memory_set;
        // 重新设置堆大小
        self.inner.exclusive_access().heap_base = uheap_base.into();
        self.inner.exclusive_access().heap_end = uheap_base.into();
        // 重新设置mmap_area
        self.inner.exclusive_access().mmap_area_base = MEMORY_MAP_BASE.into();
        self.inner.exclusive_access().mmap_area_end = MEMORY_MAP_BASE.into();
        // then we alloc user resource for main thread again
        // since memory_set has been changed
        let task = self.inner_exclusive_access().get_task(0);
        let mut task_inner = task.inner_exclusive_access();
        task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
        task_inner.res.as_mut().unwrap().alloc_user_res();
        task_inner.trap_cx_ppn = task_inner.res.as_mut().unwrap().trap_cx_ppn();
        // push arguments on user stack
        let mut user_sp = task_inner.res.as_mut().unwrap().ustack_top();
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                translated_refmut(
                    new_token,
                    (argv_base + arg * core::mem::size_of::<usize>()) as *mut usize,
                )
            })
            .collect();
        *argv[args.len()] = 0;
        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            *argv[i] = user_sp;
            let mut p = user_sp;
            for c in args[i].as_bytes() {
                *translated_refmut(new_token, p as *mut u8) = *c;
                p += 1;
            }
            *translated_refmut(new_token, p as *mut u8) = 0;
        }
        // make the user_sp aligned to 8B for k210 platform
        user_sp -= user_sp % core::mem::size_of::<usize>();
        // initialize trap_cx
        let mut trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            task.kstack.get_top(),
            trap_handler as usize,
        );
        trap_cx.x[10] = args.len();
        trap_cx.x[11] = argv_base;
        *task_inner.get_trap_cx() = trap_cx;
    }

    /// Only support processes with a single thread.
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        let mut parent = self.inner_exclusive_access();
        assert_eq!(parent.thread_count(), 1);
        // clone parent's memory_set completely including trampoline/ustacks/trap_cxs
        let memory_set = MemorySet::from_existed_user(&parent.memory_set);
        // alloc a pid
        let pid = pid_alloc();
        // copy fd table
        let mut new_fd_table: Vec<Option<FileDescriptor>> = Vec::new();
        for fd in parent.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        // copy work path
        let path = parent.work_path.clone();
        // create child process pcb
        let child = Arc::new(Self {
            pid,
            inner: unsafe {
                UPIntrFreeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                    work_path: path,
                    // cwd: path,
                    heap_base: parent.heap_base,
                    heap_end: parent.heap_end,
                    mmap_area_base: parent.mmap_area_base,
                    mmap_area_end: parent.mmap_area_end,
                })
            },
        });
        // add child
        parent.children.push(Arc::clone(&child));
        // create main thread of child process
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&child),
            parent
                .get_task(0)
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .ustack_base(),
            // here we do not allocate trap_cx or ustack again
            // but mention that we allocate a new kstack here
            false,
        ));
        // attach task to child process
        let mut child_inner = child.inner_exclusive_access();
        child_inner.tasks.push(Some(Arc::clone(&task)));
        drop(child_inner);
        // modify kstack_top in trap_cx of this thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        trap_cx.kernel_sp = task.kstack.get_top();
        drop(task_inner);
        insert_into_pid2process(child.getpid(), Arc::clone(&child));
        // add this thread to scheduler
        add_task(task);
        child
    }

    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}

//线程当前工作目录
//以目录或者文件为单位分割,便于相对路径的修改
//绝对路径
//相对路径 (处理. 和 ..)
#[derive(Clone)]
pub struct WorkPath {
    pub path: Vec<String>,
}

impl WorkPath {
    //只有init proc使用,其他线程 clone自父线程
    pub fn new() -> Self {
        Self {
            path: vec![String::from("/")],
        }
    }

    //依据输入的path更新路径
    pub fn modify_path(&mut self, input_path: &str) {
        #[inline]
        fn split(work_path: &mut WorkPath, path: &str) -> Vec<String> {
            let split_path: Vec<&str> = path.split('/').collect();
            let mut vec = vec![];
            for part_path in split_path {
                match part_path {
                    "" | "." => (),
                    ".." => {
                        work_path.path.pop();
                    }
                    part => vec.push(part.to_string()),
                    _ => (),
                }
            }
            vec
        };
        let path_vec = split(self, input_path);
        if WorkPath::is_abs_path(input_path) {
            //绝对路径补上根目录"/"
            self.path = vec![String::from("/")];
        }
        //如果是绝对路径,当前path中只包含"/"
        //如果是相对路径,当前path中为处理过.和..的path
        //和split生成的路径合并得到新路径
        self.path.extend_from_slice(&path_vec);
    }

    pub fn is_abs_path(path: &str) -> bool {
        if path.contains("^/") {
            true
        } else {
            false
        }
    }
}

use core::fmt::{Display, Formatter};

impl Display for WorkPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut path = String::new();
        for path_part in self.path.iter() {
            path.push_str(path_part.as_str());
        }
        write!(f, "{}", path)
    }
}
