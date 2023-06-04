
use super::info::{S_IFCHR, S_IFDIR, S_IFREG};
use super::{Dirent, File, Kstat};


#[allow(unused)]
use crate::drivers::BLOCK_DEVICE;
use crate::mm::UserBuffer;

use _core::str::FromStr;
use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use bitflags::*;
use lazy_static::*;
use fatfs::{create_root_vfile, FAT32Manager, VFile, ATTR_ARCHIVE, ATTR_DIRECTORY};
use log::info;
use spin::Mutex;

/// 表示进程中一个被打开的常规文件或目录
pub struct OSInode {
    readable: bool, // 该文件是否允许通过 sys_read 进行读
    writable: bool, // 该文件是否允许通过 sys_write 进行写
    inner: Mutex<OSInodeInner>,
    path: String, // todo
    name: String,
}

pub struct OSInodeInner {
    offset: usize, // 偏移量
    inode: Arc<VFile>,
    flags: OpenFlags,
    available: bool,
}

impl OSInode {
    pub fn new(readable: bool, writable: bool, inode: Arc<VFile>, path: String, name: String) -> Self {
        let available = true;
        Self {
            readable,
            writable,
            inner: Mutex::new(OSInodeInner {
                offset: 0,
                inode,
                flags: OpenFlags::empty(),
                available,
            }),
            path,
            name,
        }
    }

    #[allow(unused)]
    pub fn read_all(&self) -> Vec<u8> {
        let mut buffer = [0u8; 512];
        let mut v: Vec<u8> = Vec::new();
        let mut inner = self.inner.lock();
        loop {
            let len = inner.inode.read_at(inner.offset, &mut buffer);
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }

    /// 用于从OSInode表示的文件中读取指定长度的数据，并将其存储在一个Vec<u8>中返回。
    #[allow(unused)]
    pub fn read_vec(&self, offset: isize, len: usize) -> Vec<u8> {
        let mut inner = self.inner.lock();
        // 用于跟踪还需要读取的数据长度
        let mut len = len;
        let old_offset = inner.offset;
        if offset >= 0 {
            inner.offset = offset as usize;
        }
        let mut buffer = [0u8; 512];
        let mut v: Vec<u8> = Vec::new();
        if len == 96 * 4096 {
            // 防止 v 占用空间过度扩大
            v.reserve(96 * 4096);
        }
        loop {
            let read_size = inner.inode.read_at(inner.offset, &mut buffer);
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            v.extend_from_slice(&buffer[..read_size.min(len)]);
            if len > read_size {
                len -= read_size;
            } else {
                break;
            }
        }
        if offset >= 0 {
            inner.offset = old_offset;
        }
        v
    }

    #[allow(unused)]
    pub fn write_all(&self, str_vec: &Vec<u8>) -> usize {
        let mut inner = self.inner.lock();
        let mut remain = str_vec.len();
        let mut base = 0;
        loop {
            let len = remain.min(512);
            inner.inode.write_at(inner.offset, &str_vec.as_slice()[base..base + len]);
            inner.offset += len;
            base += len;
            remain -= len;
            if remain == 0 {
                break;
            }
        }
        base
    }

    pub fn is_dir(&self) -> bool {
        let inner = self.inner.lock();
        inner.inode.is_dir()
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn delete(&self) -> usize {
        let inner = self.inner.lock();
        inner.inode.remove()
    }
    pub fn file_size(&self) -> usize {
        let inner = self.inner.lock();
        inner.inode.file_size() as usize
    }
    #[allow(unused)]
    pub fn set_head_cluster(&self, cluster: u32) {
        let inner = self.inner.lock();
        let vfile = &inner.inode;
        vfile.set_first_cluster(cluster);
    }
    #[allow(unused)]
    pub fn get_head_cluster(&self) -> u32 {
        let inner = self.inner.lock();
        let vfile = &inner.inode;
        vfile.first_cluster()
    }
}

// 这里在实例化的时候进行文件系统的打开
lazy_static! {
    pub static ref ROOT_INODE: Arc<VFile> = {
        let fat32_manager = FAT32Manager::open(BLOCK_DEVICE.clone());
        Arc::new(create_root_vfile(&fat32_manager)) // 返回根目录
    };
}

pub fn init() {
    // 预创建文件/文件夹
    // open("/", "proc", OpenFlags::O_DIRECTROY | OpenFlags::O_CREATE);
    // open("/", "tmp", OpenFlags::O_DIRECTROY | OpenFlags::O_CREATE);
    // open("/", "dev", OpenFlags::O_DIRECTROY | OpenFlags::O_CREATE);
    // open("/", "var", OpenFlags::O_DIRECTROY | OpenFlags::O_CREATE);
    // open("/dev", "misc", OpenFlags::O_DIRECTROY | OpenFlags::O_CREATE);
    // open("/var", "tmp", OpenFlags::O_DIRECTROY | OpenFlags::O_CREATE);
    // open("/dev", "null", OpenFlags::O_CREATE);
    // open("/dev", "zero", OpenFlags::O_CREATE);
    // open("/proc", "mounts", OpenFlags::O_CREATE);
    // open("/proc", "meminfo", OpenFlags::O_CREATE);
    // open("/dev/misc", "rtc", OpenFlags::O_CREATE);
    // open("/var/tmp", "lmbench", OpenFlags::O_CREATE);
    println!("/**** All Files  ****");
    list_apps(ROOT_INODE.clone());
    println!("**********************/");
}

static mut LAYER: usize = 0;
pub fn list_apps(dir: Arc<VFile>) {
    for app in dir.ls().unwrap() {
        // 不打印initproc，事实上它也在task::new之后删除了
        unsafe {
            if LAYER == 0 && app.0 == "initproc" {
                continue;
            }
        }
        if app.1 & ATTR_DIRECTORY == 0 {
            // 如果不是目录
            unsafe {
                for _ in 0..LAYER {
                    print!("----");
                }
            }
            println!("{}", app.0);
        } else if app.0 != "." && app.0 != ".." {
            unsafe {
                for _ in 0..LAYER {
                    print!("----");
                }
            }
            info!("{}/", app.0);
            let dir = open(dir.name(), app.0.as_str(), OpenFlags::O_RDONLY).unwrap();
            let inner = dir.inner.lock();
            let inode = inner.inode.clone();
            unsafe {
                LAYER += 1;
            }
            list_apps(inode);
        }
    }
    unsafe {
        LAYER -= 1;
    }
}

// 定义一份打开文件的标志
bitflags! {
    pub struct OpenFlags: u32 {
        const O_RDONLY    = 0;
        const O_WRONLY    = 1 << 0;
        const O_RDWR      = 1 << 1;
        const O_CREATE    = 1 << 6;
        const O_EXCL      = 1 << 7;
        const O_TRUNC     = 1 << 9;
        const O_APPEND    = 1 << 10;
        const O_NONBLOCK  = 1 << 11;
        const O_LARGEFILE = 1 << 15;
        const O_DIRECTROY = 1 << 16;
        const O_NOFOLLOW  = 1 << 17;
        const O_CLOEXEC   = 1 << 19;
    }
}

impl OpenFlags {
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::O_WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

/// 根据传入的路径、标志位等参数打开文件，并返回一个表示该文件的Arc<OSInode>实例。
pub fn open(work_path: &str, path: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    // println!("[DEBUG] enter open: work_path:{}, path:{}, flags:{:?}", work_path, path, flags);
    // 存储路径各个部分的字符串切片
    let mut pathv: Vec<&str> = path.split('/').collect();
    // println!("pathv:{:?}",pathv);
    // 根据work_path（工作路径）查找到当前的cur_inode（当前索引节点）。如果work_path是根目录"/"，则将ROOT_INODE克隆为cur_inode；
    // 否则，将work_path按照'/'分割成字符串切片，然后通过ROOT_INODE逐级查找到对应的索引节点。
    let cur_inode = {
        if work_path == "/" {
            ROOT_INODE.clone()
        } else {
            let wpath: Vec<&str> = work_path.split('/').collect();
            ROOT_INODE.find_vfile_bypath(wpath).unwrap()
        }
    };
    // 根据flags参数获取可读性和可写性的标志位。
    let (readable, writable) = flags.read_write();
    // 检查是否包含O_CREATE标志位。如果包含，则表示需要创建文件。
    if flags.contains(OpenFlags::O_CREATE) {
        if let Some(inode) = cur_inode.find_vfile_bypath(pathv.clone()) {
            // 如果文件已存在，则清空该文件，
            // 并根据work_path、文件名等参数创建一个新的OSInode实例，
            // 将其包装为Arc并返回。
            let name = pathv.pop().unwrap();
            inode.clear();
            Some(Arc::new(OSInode::new(
                readable,
                writable,
                inode,
                work_path.to_string(),
                name.to_string(),
            )))
        } else {
            // 果文件不存在，则根据pathv、create_type等参数创建一个新文件，并返回相应的OSInode实例。
            let mut create_type = ATTR_ARCHIVE;
            if flags.contains(OpenFlags::O_DIRECTROY) {
                create_type = ATTR_DIRECTORY;
            }
            let name = pathv.pop().unwrap();
            if let Some(temp_inode) = cur_inode.find_vfile_bypath(pathv.clone()) {
                // println!("[DEBUG] create file: {}, type:0x{:x}",name,create_type);
                temp_inode
                    .create(name, create_type)
                    .map(|inode| Arc::new(OSInode::new(readable, writable, inode, work_path.to_string(), name.to_string())))
            } else {
                None
            }
        }
    } else {
        // 表示不需要创建文件。根据cur_inode和pathv查找到对应的文件索引节点，根据flags中的O_TRUNC标志位决定是否清空文件内容，并返回相应的OSInode实例。
        cur_inode.find_vfile_bypath(pathv).map(|inode| {
            if flags.contains(OpenFlags::O_TRUNC) {
                inode.clear();
            }
            let name = inode.name().to_string();
            Arc::new(OSInode::new(readable, writable, inode, work_path.to_string(), name))
        })
    }
}

/// 作用是根据传入的当前工作路径和目标路径，更改当前工作目录，并返回新的工作路径。
/// 如果找到目标目录，则将当前工作目录和目标路径拼接为新的工作路径；
/// 如果未找到目标目录，则返回None表示操作失败。
pub fn chdir(work_path: &str, path: &str) -> Option<String> {
    // 根据传入的path和work_path参数获取当前的目录索引节点current_inode。
    let current_inode = {
        if path.chars().nth(0).unwrap() == '/' {
            // 如果path的第一个字符是'/'，则表示传入的路径是绝对路径，直接将ROOT_INODE克隆为current_inode
            ROOT_INODE.clone()
        } else {
            // 表示传入的路径是相对路径，将work_path按照'/'分割成字符串切片，并通过ROOT_INODE逐级查找到对应的索引节点。
            let current_work_pathv: Vec<&str> = work_path.split('/').collect();
            ROOT_INODE.find_vfile_bypath(current_work_pathv).unwrap()
        }
    };
    // 将path按照'/'分割成字符串切片pathv，用于后续查找目标目录。
    let pathv: Vec<&str> = path.split('/').collect();
    // 检查是否找到了目标目录
    if let Some(_) = current_inode.find_vfile_bypath(pathv) {
        let new_current_path = String::from_str("/").unwrap() + &String::from_str(path).unwrap();
        if current_inode.name() == "/" {
            Some(new_current_path)
        } else {
            Some(String::from_str(current_inode.name()).unwrap() + &new_current_path)
        }
    } else {
        None
    }
}

// 为 OSInode 实现 File Trait
impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }

    fn writable(&self) -> bool {
        self.writable
    }

    fn available(&self) -> bool {
        let inner = self.inner.lock();
        inner.available
    }

    fn read(&self, mut buf: UserBuffer) -> usize {
        // println!("osinode read, current offset:{}",self.inner.lock().offset);
        let offset = self.inner.lock().offset;
        let file_size = self.file_size();
        if file_size == 0 {
            println!("[WARNING] OSinode read: file_size is zero!");
        }
        if offset >= file_size {
            return 0;
        }
        let mut inner = self.inner.lock();
        let mut total_read_size = 0usize;

        // 这边要使用 iter_mut()，因为要将数据写入
        for slice in buf.buffers.iter_mut() {
            let read_size = inner.inode.read_at(inner.offset, *slice);
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        total_read_size
    }

    fn read_kernel_space(&self) -> Vec<u8> {
        let file_size = self.file_size();
        let mut inner = self.inner.lock();
        let mut buffer = [0u8; 512];
        let mut v: Vec<u8> = Vec::new();
        loop {
            if inner.offset > file_size {
                break;
            }
            let readsize = inner.inode.read_at(inner.offset, &mut buffer);
            if readsize == 0 {
                break;
            }
            inner.offset += readsize;
            v.extend_from_slice(&buffer[..readsize]);
        }
        v.truncate(v.len().min(file_size));
        v
    }

    fn write(&self, buf: UserBuffer) -> usize {
        let mut total_write_size = 0usize;
        let filesize = self.file_size();
        let mut inner = self.inner.lock();
        if inner.flags.contains(OpenFlags::O_APPEND) {
            for slice in buf.buffers.iter() {
                let write_size = inner.inode.write_at(filesize, *slice);
                inner.offset += write_size;
                total_write_size += write_size;
            }
        } else {
            for slice in buf.buffers.iter() {
                let write_size = inner.inode.write_at(inner.offset, *slice);
                assert_eq!(write_size, slice.len());
                inner.offset += write_size;
                total_write_size += write_size;
            }
        }
        total_write_size
    }

    fn write_kernel_space(&self, data: Vec<u8>) -> usize {
        let mut inner = self.inner.lock();
        let mut remain = data.len();
        let mut base = 0;
        loop {
            let len = remain.min(512);
            inner.inode.write_at(inner.offset, &data.as_slice()[base..base + len]);
            inner.offset += len;
            base += len;
            remain -= len;
            if remain == 0 {
                break;
            }
        }
        base
    }

    fn get_name(&self) -> &str {
        self.name()
    }

    fn get_offset(&self) -> usize {
        let inner = self.inner.lock();
        inner.offset
    }

    fn set_offset(&self, offset: usize) {
        let mut inner = self.inner.lock();
        inner.offset = offset;
    }

    fn set_flags(&self, flag: OpenFlags) {
        let mut inner = self.inner.lock();
        inner.flags.set(flag, true);
    }

    fn set_cloexec(&self) {
        let mut inner = self.inner.lock();
        inner.available = false;
    }

    fn get_dirent(&self, dirent: &mut Dirent) -> isize {
        if !self.is_dir() {
            return -1;
        }
        let mut inner = self.inner.lock();
        let offset = inner.offset as u32;
        if let Some((name, off, first_clu, _attr)) = inner.inode.dirent_info(offset as usize) {
            dirent.init(name.as_str(), off as isize, first_clu as usize);
            inner.offset = off as usize;
            let len = (name.len() + 8 * 4) as isize;
            len
        } else {
            -1
        }
    }

    fn get_fstat(&self, kstat: &mut Kstat) {
        let inner = self.inner.lock();
        let vfile = inner.inode.clone();
        let mut st_mode = 0;
        _ = st_mode;
        let (st_size, st_blksize, st_blocks, is_dir, time) = vfile.stat();
        if is_dir {
            st_mode = S_IFDIR;
        } else {
            st_mode = S_IFREG;
        }
        if vfile.name() == "null" || vfile.name() == "zero" {
            st_mode = S_IFCHR;
        }
        kstat.init(st_size, st_blksize as i32, st_blocks, st_mode, time);
    }

    fn file_size(&self) -> usize {
        self.file_size()
    }

    fn get_path(&self) -> &str {
        self.path.as_str()
    }
}

