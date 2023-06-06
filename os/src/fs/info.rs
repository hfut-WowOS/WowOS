#![allow(unused)]

/// os/src/fs/info
/// 定义了两个结构体：Stat和Statfs，用于表示文件的状态和文件系统的状态。
///
/// 定义了一个用于存储目录中文件信息的Dirent结构体。
/// 用于存储目录中的文件信息，包括文件的索引节点号、偏移量、长度、类型和文件名。
/// as_bytes方法提供了将结构体转换为字节数组的功能，方便在操作系统中进行数据传输和存储。
///
use core::slice::from_raw_parts;

use fatfs::BLOCK_SZ;

pub const DTYPE_DIR: u8 = 4;
pub const DTYPE_REG: u8 = 8;
pub const DTYPE_UNKNOWN: u8 = 0;

#[repr(C)]
pub struct Dirent {
    d_ino: u64,
    offset: i64,
    dirent_len: u16,
    d_type: u8,
    d_name: [u8; 128],
}

impl Dirent {
    pub fn default() -> Self {
        Self {
            d_ino: 0,
            offset: 0,
            dirent_len: 0,
            d_type: 0,
            d_name: [0; 128],
        }
    }

    pub fn new(name: &str, inode_id: u64, offset: i64, dirent_len: u16, d_type: u8) -> Self {
        Self {
            d_ino: inode_id,
            offset,
            dirent_len,
            d_type,
            d_name: Dirent::str2bytes(name),
        }
    }

    pub fn fill_info(&mut self, name: &str, inode: u64, offset: i64, dirent_len: u16, d_type: u8) {
        *self = Self {
            d_ino: inode,
            offset,
            dirent_len,
            d_type,
            d_name: Self::str2bytes(name),
        };
    }

    fn str2bytes(str: &str) -> [u8; 128] {
        let bytes = str.as_bytes();
        let len = bytes.len();
        assert!(len <= 128);
        let mut buf = [0u8; 128];
        let copy_part = &mut buf[..len];
        copy_part.copy_from_slice(bytes);
        buf
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Dirent>();
        unsafe { from_raw_parts(self as *const _ as *const u8, size) }
    }
}

bitflags! {
    pub struct VFSFlag:u32{
        //file type mask
        const S_IFMT    = 0170_000; /*mask*/

        const S_IFSOCK  = 0140_000; /*socket file*/
        const S_IFLNK   = 0120_000; /*link file*/
        const S_IFREG   = 0100_000; /*regular file*/
        const S_IFBLK   = 0060_000; /*block device*/
        const S_IFDIR   = 0040_000; /*directory*/
        const S_IFCHR   = 0020_000; /*char device*/
        const S_IFIFO   = 0010_000; /*fifo*/

        //file mode mask
        const S_ISUID   = 04000; /*set uid*/
        const S_ISGID   = 02000; /*set gid*/
        const S_ISVTX   = 01000; /*stick bit*/

        //use
        const S_IRWXU   = 00700; /*read write execute*/
        const S_IRUSR   = 00400; /*read*/
        const S_IWUSR   = 00200; /*write*/
        const S_IXUSR   = 00100; /*exec*/

        //group
        const S_IRWXG   = 00070; /**/
        const S_IRGRP   = 00040;
        const S_IWGRP   = 00020;
        const S_IXGRP   = 00010;
    }
}

impl VFSFlag {
    pub fn create_flag(file_type: VFSFlag, user_perm: VFSFlag, group_perm: VFSFlag) -> Self {
        unsafe { VFSFlag::from_bits_unchecked((file_type | user_perm | group_perm).bits) }
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
