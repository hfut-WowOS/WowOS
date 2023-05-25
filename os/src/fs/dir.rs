use alloc::string::String;
// use alloc::vec::Vec;
use core::mem::size_of;
use core::slice::{from_raw_parts, from_raw_parts_mut};

// 文件名的最大长度
pub const NAME_LIMIE: usize = 128;

pub const DT_UNKNOWN: u8 = 0; // 未知类型
pub const DT_DIR: u8 = 4; // 目录
pub const DT_REG: u8 = 8; // 普通文件

#[derive(Debug)]
#[repr(C)]
pub struct DirEntry {
    pub inode: usize,           // 索引结点号
    pub offset: isize,          // 64-bit offset to next structure
    pub reclen: u16,            // Size of this dirent
    pub dtype: u8,              // 类型
    pub name: [u8; NAME_LIMIE], // 文件名
}

impl DirEntry {
    pub fn new(inode: usize, offset: isize, dtype: u8, name: String) -> Self {
        Self {
            inode: inode,
            offset: offset,
            dtype: dtype,
            reclen: name.len() as u16,
            name: {
                let mut tmp: [u8; NAME_LIMIE] = [0; NAME_LIMIE];
                tmp[..name.len()].copy_from_slice(name.as_bytes());
                tmp
            },
        }
    }

    pub fn empty() -> Self {
        Self {
            inode: 0,
            offset: 0,
            reclen: 0,
            dtype: size_of::<Self>() as u8,
            name: [0; NAME_LIMIE],
        }
    }

    pub fn set(&mut self, name: &str, inode: usize, offset: isize, reclen: u16, dtype: u8) {
        *self = Self {
            inode: inode,
            offset: offset,
            reclen: reclen,
            dtype: dtype,
            name: self.name,
        };
        self.set_name(name);
    }

    pub fn set_name(&mut self, name: &str) {
        let len = name.len().min(NAME_LIMIE);
        let name_bytes = name.as_bytes();
        for i in 0..len {
            self.name[i] = name_bytes[i]
        }
        self.name[len] = 0;
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = size_of::<Self>();
        unsafe { from_raw_parts(self as *const _ as usize as *const u8, size) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        let size = size_of::<Self>();
        unsafe { from_raw_parts_mut(self as *mut _ as usize as *mut u8, size) }
    }
}

#[repr(C)]
pub struct Kstat {
    st_dev: u64,
    /* ID of device containing file */
    st_ino: u64,
    /* Inode number */
    st_mode: u32,
    /* File type and mode */
    st_nlink: u32,
    /* Number of hard links */
    st_uid: u32,
    st_gid: u32,
    st_rdev: u64,
    __pad: u64,
    st_size: u32,
    st_blksize: u32,
    __pad2: i32,
    st_blocks: u64,
    st_atime_sec: i64,
    st_atime_nsec: i64,
    st_mtime_sec: i64,
    st_mtime_nsec: i64,
    st_ctime_sec: i64,
    st_ctime_nsec: i64,
}

impl Default for Kstat {
    fn default() -> Self {
        let flags = VFSFlag::create_flag(VFSFlag::S_IFREG, VFSFlag::S_IRWXU, VFSFlag::S_IRWXG);
        Self {
            st_dev: 0,
            st_ino: 0,
            st_mode: flags.bits,
            st_nlink: 1,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            __pad: 0,
            st_size: 0,
            st_blksize: BLOCK_SZ as u32,
            __pad2: 0,
            st_blocks: 0,
            st_atime_sec: 0,
            st_atime_nsec: 0,
            st_mtime_sec: 0,
            st_mtime_nsec: 0,
            st_ctime_sec: 0,
            st_ctime_nsec: 0,
        }
    }
}

impl Kstat {
    pub fn update(
        &mut self,
        st_ino: u64,
        st_mode: u32,
        st_size: u32,
        access_time: i64,
        modify_time: i64,
        create_time: i64,
    ) {
        *self = Kstat {
            st_ino,
            st_mode,
            st_size,
            st_atime_sec: access_time,
            st_mtime_sec: modify_time,
            st_ctime_sec: create_time,
            ..*self
        }
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Self>();
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, size) }
    }
}