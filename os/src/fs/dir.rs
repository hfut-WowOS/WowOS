use alloc::vec::Vec;
use core::mem::size_of;
use core::slice::{from_raw_parts, from_raw_parts_mut};
use alloc::string::String;

// 文件名的最大长度
pub const NAME_LIMIE: usize = 128;

pub const DT_UNKNOWN: u8 = 0;   // 未知类型
pub const DT_DIR: u8 = 4;       // 目录
pub const DT_REG: u8 = 8;       // 普通文件

#[derive(Debug)]
#[repr(C)]
pub struct DirEntry {
    pub inode: usize,   // 索引结点号
    pub offset: isize,  // 64-bit offset to next structure
    pub reclen: u16,    // Size of this dirent
    pub dtype: u8,      // 类型
    pub name: [u8; NAME_LIMIE],
}

impl DirEntry {

    pub fn new(inode: usize, offset: isize, dtype: u8, name: String) -> Self {
        Self {
            inode: inode,
            offset: offset,
            reclen: name.len() as u16,
            name: {
                let mut tmp: [u8; MAXNAME] = [0; MAXNAME];
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