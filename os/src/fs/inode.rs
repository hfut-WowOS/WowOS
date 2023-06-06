use core::str::FromStr;

use super::*;
use crate::drivers::BLOCK_DEVICE;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use fatfs::*;
use lazy_static::lazy_static;
use log::info;
use spin::Mutex;
use crate::fs::info::{VFSFlag, DTYPE_DIR, DTYPE_REG, DTYPE_UNKNOWN};

/// 表示进程中一个被打开的常规文件或目录
pub struct OSInode {
    readable: bool,
    writable: bool,
    inner: Mutex<OsInodeInner>,
}

pub struct OsInodeInner {
    offset: usize,
    inode: Arc<VFile>,
}

impl OSInode {
    pub fn new(readable: bool, writable: bool, inode: Arc<VFile>) -> Self {
        Self {
            readable,
            writable,
            inner: Mutex::new(OsInodeInner { offset: 0, inode }),
        }
    }

    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.lock();
        let mut buf = [0u8; BLOCK_SZ];
        let mut vec = Vec::new();

        loop {
            //分块读取文件内容
            let size = inner.inode.read_at(inner.offset, &mut buf);
            if size == 0 {
                break;
            }
            inner.offset += size;
            vec.extend_from_slice(&buf[..size]);
        }

        vec
    }

    pub fn is_dir(&self) -> bool {
        let inner = self.inner.lock();
        inner.inode.is_dir()
    }

    #[allow(unused)]
    pub fn clear(&self) {
        let inner = self.inner.lock();
        inner.inode.clear()
    }

    pub fn delete(&self) -> usize {
        self.inner.lock().inode.remove()
    }

    pub fn get_size(&self) -> usize {
        self.inner.lock().inode.get_size() as usize
    }

    pub fn set_offset(&self, offset: usize) {
        self.inner.lock().offset = offset;
    }

    //在当前目录下创建文件
    pub fn create(&self, path: &str, _type: FileType) -> Option<Arc<OSInode>> {
        let inode = self.inner.lock().inode.clone();
        if !inode.is_dir() {
            println!("It's not a directory");
            return None;
        }
        let mut path_split: Vec<&str> = path.split('/').collect();
        let (readable, writable) = (true, true);
        if let Some(target_inode) = inode.find_vfile_bypath(path_split.clone()) {
            target_inode.remove();
        }
        let filename = path_split.pop().unwrap();

        //创建失败的条件包括: 目录不存在,存在文件但不是目录
        match inode.find_vfile_bypath(path_split) {
            None => None,
            Some(vfile) => {
                if !vfile.is_dir() {
                    None
                } else {
                    let attr = _type.into();
                    vfile
                        .create(filename, attr)
                        .map(|inode| Arc::new(OSInode::new(readable, writable, inode)))
                }
            }
        }
    }

    pub fn find(&self, path: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
        let inner = self.inner.lock();
        let path_split = path.split('/').collect();

        inner.inode.find_vfile_bypath(path_split).map(|inode| {
            let (readable, writable) = flags.read_write();
            Arc::new(OSInode::new(readable, writable, inode.clone()))
        })
    }

    pub fn get_dirent(&self, dirent: &mut Dirent) -> isize {
        let mut inner = self.inner.lock();
        if let Some((name, offset, first_clu, attr)) = inner.inode.dirent_info(inner.offset) {
            let d_type = if attr & ATTRIBUTE_ARCHIVE != 0 {
                DTYPE_REG
            } else if attr & ATTRIBUTE_DIRECTORY != 0 {
                DTYPE_DIR
            } else {
                DTYPE_UNKNOWN
            };

            dirent.fill_info(
                &name,
                first_clu as u64,
                offset as i64,
                (offset as usize - inner.offset) as u16,
                d_type,
            );

            inner.offset = offset as usize;
            let len = (name.len() + 8 * 4) as isize;
            len
        } else {
            -1
        }
    }

    pub fn get_fstat(&self, fstat: &mut Kstat) {
        let vfile = self.inner.lock().inode.clone();

        let (size, access_t, modify_t, create_t, inode_num) = vfile.stat();
        let st_mode = {
            if vfile.is_dir() {
                VFSFlag::create_flag(VFSFlag::S_IFDIR, VFSFlag::S_IRWXU, VFSFlag::S_IRWXG)
            } else {
                VFSFlag::create_flag(VFSFlag::S_IFREG, VFSFlag::S_IRWXU, VFSFlag::S_IRWXG)
            }
        }
        .bits();

        fstat.update(
            inode_num,
            st_mode,
            size as u32,
            access_t,
            modify_t,
            create_t,
        );
    }
}

// 这里在实例化的时候进行文件系统的打开
lazy_static! {
    pub static ref ROOT_INODE: Arc<VFile> = {
        let fat_manager = FAT32Manager::open(BLOCK_DEVICE.clone());
        let reader = fat_manager.read();
        Arc::new(reader.get_root_vfile(&fat_manager))
    };
}

pub fn init() {
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
        if app.1 & 0x10 == 0 {
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
            let dir = open_file(dir.get_name(), app.0.as_str(), OpenFlags::O_RDONLY, FileType::Dir).unwrap();
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

pub enum FileType {
    Dir,
    Regular,
}

impl Into<u8> for FileType {
    fn into(self) -> u8 {
        match self {
            FileType::Dir => ATTRIBUTE_DIRECTORY,
            FileType::Regular => ATTRIBUTE_ARCHIVE,
        }
    }
}

//path可以是基于当前工作路径的相对路径或者绝对路径
pub fn open_file(
    work_path: &str,
    path: &str,
    flags: OpenFlags,
    _type: FileType,
) -> Option<Arc<OSInode>> {
    let cur_inode = get_current_inode(work_path);
    let (readable, writable) = flags.read_write();
    let mut path_split: Vec<&str> = path.split('/').collect();

    //创建文件
    if flags.contains(OpenFlags::O_CREATE) {
        //如果文件存在删除对应文件
        if let Some(inode) = cur_inode.find_vfile_bypath(path_split.clone()) {
            inode.remove();
        }

        let filename = path_split.pop().unwrap();
        let dir = cur_inode.find_vfile_bypath(path_split).unwrap();
        let attr = _type.into();
        //创建文件
        dir.create(filename, attr)
            .map(|inode| Arc::new(OSInode::new(readable, writable, inode)))
    } else {
        cur_inode.find_vfile_bypath(path_split).map(|inode| {
            if flags.contains(OpenFlags::O_TRUNC) {
                inode.clear()
            }
            Arc::new(OSInode::new(readable, writable, inode))
        })
    }
}

#[inline]
pub fn ch_dir(curr_path: &str, path: &str) -> isize {
    let curr_inode = get_current_inode(curr_path);
    let path_split: Vec<&str> = path.split("/").collect();
    match curr_inode.find_vfile_bypath(path_split) {
        None => -1,
        Some(inode) => {
            let attribute = inode.get_attribute();
            if attribute == ATTRIBUTE_DIRECTORY || attribute == ATTRIBUTE_LFN {
                0
            } else {
                -1
            }
        }
    }
}

/// 作用是根据传入的当前工作路径和目标路径，更改当前工作目录，并返回新的工作路径。
/// 如果找到目标目录，则将当前工作目录和目标路径拼接为新的工作路径；
/// 如果未找到目标目录，则返回None表示操作失败。
#[allow(unused)]
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
        if current_inode.get_name() == "/" {
            Some(new_current_path)
        } else {
            Some(String::from_str(current_inode.get_name()).unwrap() + &new_current_path)
        }
    } else {
        None
    }
}

impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }

    fn writable(&self) -> bool {
        self.writable
    }

    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.lock();
        let mut read_size = 0;
        for slice in buf.buffers.iter_mut() {
            let size = inner.inode.read_at(inner.offset, *slice);
            if size == 0 {
                break;
            }
            inner.offset += size;
            read_size += size;
        }

        read_size
    }

    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.lock();
        let write_size = 0;
        for buffer in buf.buffers.iter() {
            let size = inner.inode.write_at(inner.offset, *buffer);
            if size == 0 {
                break;
            }
            inner.offset += size;
            let _ = write_size + size;
        }
        write_size
    }
}
