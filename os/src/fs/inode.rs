use alloc::sync::Arc;
use alloc::vec::Vec;

use fat32::{FAT32Manager, VFile};
use fat32::{ATTRIBUTE_ARCHIVE, ATTRIBUTE_DIRECTORY, ATTRIBUTE_LFN, BLOCK_SZ};
use lazy_static::lazy_static;
use spin::Mutex;

use crate::drivers::BLOCK_DEVICE;
use super::*;
use crate::mm::UserBuffer;
// use crate::task::current_user_token;

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

    pub fn get_dirent(&self, dirent: &mut dirent) -> isize {
        let mut inner = self.inner.lock();
        if let Some((name, offset, first_clu, attr)) = inner.inode.dirent_info(inner.offset) {
            let d_type = if attr & ATTRIBUTE_ARCHIVE != 0 {
                DT_REG
            } else if attr & ATTRIBUTE_DIRECTORY != 0 {
                DT_DIR
            } else {
                DT_UNKNOWN
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
        let mut write_size = 0;
        for buffer in buf.buffers.iter() {
            let size = inner.inode.write_at(inner.offset, *buffer);
            if size == 0 {
                break;
            }
            inner.offset += size;
            write_size + size;
        }
        write_size
    }
}

pub fn get_current_inode(curr_path: &str) -> Arc<VFile> {
    if curr_path == "/" || curr_path.contains("^/") {
        ROOT_INODE.clone()
    } else {
        let path: Vec<&str> = curr_path.split("/").collect();
        ROOT_INODE.find_vfile_bypath(path).unwrap()
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

lazy_static! {
    pub static ref ROOT_INODE: Arc<VFile> = {
        let fat_manager = FAT32Manager::open(BLOCK_DEVICE.clone());
        let reader = fat_manager.read();
        Arc::new(reader.get_root_vfile(&fat_manager))
    };
}

bitflags! {
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RDWR = 1 << 1;
        const CREATE = 1 << 6;
        const TRUNC = 1 << 10;
        const DIRECTROY = 0200000;
        const LARGEFILE  = 0100000;
        const CLOEXEC = 02000000;
    }
}

impl OpenFlags {
    //(readable,writable)
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

pub fn list_apps() {
    println!("/**** APPS ****");
    for app in ROOT_INODE.ls_lite().unwrap() {
        println!("{}", app.0);
    }
    println!("**************/")
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
    if flags.contains(OpenFlags::CREATE) {
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
            if flags.contains(OpenFlags::TRUNC) {
                inode.clear()
            }
            Arc::new(OSInode::new(readable, writable, inode))
        })
    }
}
