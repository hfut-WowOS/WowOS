#![allow(unused)]

/// os/src/fs/info
/// 定义了两个结构体：Stat和Statfs，用于表示文件的状态和文件系统的状态。
///
/// 定义了一个用于存储目录中文件信息的Dirent结构体。
/// 用于存储目录中的文件信息，包括文件的索引节点号、偏移量、长度、类型和文件名。
/// as_bytes方法提供了将结构体转换为字节数组的功能，方便在操作系统中进行数据传输和存储。
///
use core::mem::size_of;

pub const S_IFMT: u32 = 0o170000; //bit mask for the file type bit field
pub const S_IFSOCK: u32 = 0o140000; //socket
pub const S_IFLNK: u32 = 0o120000; //symbolic link
pub const S_IFREG: u32 = 0o100000; //regular file
pub const S_IFBLK: u32 = 0o060000; //block device
pub const S_IFDIR: u32 = 0o040000; //directory
pub const S_IFCHR: u32 = 0o020000; //character device
pub const S_IFIFO: u32 = 0o010000; //FIFO

pub const S_ISUID: u32 = 0o4000; //set-user-ID bit (see execve(2))
pub const S_ISGID: u32 = 0o2000; //set-group-ID bit (see below)
pub const S_ISVTX: u32 = 0o1000; //sticky bit (see below)

pub const S_IRWXU: u32 = 0o0700; //owner has read, write, and execute permission
pub const S_IRUSR: u32 = 0o0400; //owner has read permission
pub const S_IWUSR: u32 = 0o0200; //owner has write permission
pub const S_IXUSR: u32 = 0o0100; //owner has execute permission

pub const S_IRWXG: u32 = 0o0070; //group has read, write, and execute permission
pub const S_IRGRP: u32 = 0o0040; //group has read permission
pub const S_IWGRP: u32 = 0o0020; //group has write permission
pub const S_IXGRP: u32 = 0o0010; //group has execute permission

pub const S_IRWXO: u32 = 0o0007; //others (not in group) have read, write,and execute permission
pub const S_IROTH: u32 = 0o0004; //others have read permission
pub const S_IWOTH: u32 = 0o0002; //others have write permission
pub const S_IXOTH: u32 = 0o0001; //others have execute permission

/// Kstat结构体用于表示文件的状态信息，包括设备ID、索引节点号、文件类型和模式、硬链接数、所有者的用户ID和组ID、
/// 设备ID（如果是特殊文件）、文件大小、文件系统I/O的块大小、分配的块数、上次访问时间、上次修改时间、上次状态变化时间等。
#[derive(Debug)]
#[repr(C)]
pub struct Kstat {
    st_dev: u64,   // 包含文件的设备 ID
    st_ino: u64,   // 索引节点号
    st_mode: u32,  // 文件类型和模式
    st_nlink: u32, // 硬链接数
    st_uid: u32,   // 所有者的用户 ID
    st_gid: u32,   // 所有者的组 ID
    st_rdev: u64,  // 设备 ID（如果是特殊文件）
    __pad: u64,
    st_size: i64,    // 总大小，以字节为单位
    st_blksize: i32, // 文件系统 I/O 的块大小
    __pad2: i32,
    st_blocks: u64,     // 分配的 512B 块数
    st_atime_sec: i64,  // 上次访问时间
    st_atime_nsec: i64, // 上次访问时间（纳秒精度）
    st_mtime_sec: i64,  // 上次修改时间
    st_mtime_nsec: i64, // 上次修改时间（纳秒精度）
    st_ctime_sec: i64,  // 上次状态变化的时间
    st_ctime_nsec: i64, // 上次状态变化的时间（纳秒精度）
    __unused: [u32; 2],
}

impl Kstat {
    pub fn new() -> Self {
        Self {
            st_dev: 0,
            st_ino: 0,
            st_mode: 0,
            st_nlink: 0,
            st_uid: 0,
            st_gid: 0,
            st_rdev: 0,
            __pad: 0,
            st_size: 0,
            st_blksize: 0,
            __pad2: 0,
            st_blocks: 0,
            st_atime_sec: 0,
            st_atime_nsec: 0,
            st_mtime_sec: 0,
            st_mtime_nsec: 0,
            st_ctime_sec: 0,
            st_ctime_nsec: 0,
            __unused: [0; 2],
        }
    }

    pub fn init(&mut self, st_size: i64, st_blksize: i32, st_blocks: u64, st_mode: u32, time: u64) {
        self.st_nlink = 1;
        self.st_size = st_size;
        self.st_blksize = st_blksize;
        self.st_blocks = st_blocks;
        self.st_mode = st_mode;
        _ = time;
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Self>();
        unsafe { core::slice::from_raw_parts(self as *const _ as usize as *const u8, size) }
    }
}

#[repr(C)]
pub struct Statfs {
    f_type: u64,
    f_bsize: u64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_fsid: u64,
    f_namelen: u64,
    f_frsize: u64,
    f_flag: u64,
    f_spare: [u64; 4],
}

impl Statfs {
    pub fn new() -> Self {
        Self {
            f_type:1,
            f_bsize: 512,
            f_blocks: 12345,
            f_bfree: 1234,
            f_bavail: 123,
            f_files: 1000,
            f_ffree: 100,
            f_fsid: 1,
            f_namelen: 123,
            f_frsize: 4096,
            f_flag: 123,
            f_spare: [0; 4],
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Self>();
        unsafe { core::slice::from_raw_parts(self as *const _ as usize as *const u8, size) }
    }
}



/// 存储目录中的文件信息
pub const NAME_LIMIT: usize = 64;

/// 存储目录中的文件信息
#[repr(C)]
#[derive(Debug)]
pub struct Dirent {
    d_ino: usize,             // 索引节点号
    d_off: isize,             // 从 0 开始到下一个 dirent 的偏移
    d_reclen: u16,            // 当前 dirent 的长度
    d_type: u8,               // 文件类型
    d_name: [u8; NAME_LIMIT], // 文件名
}

impl Dirent {
    pub fn new() -> Self {
        Self {
            d_ino: 0,
            d_off: 0,
            d_reclen: core::mem::size_of::<Self>() as u16,
            d_type: 0,
            d_name: [0; NAME_LIMIT],
        }
    }

    pub fn init(&mut self, name: &str, offset: isize, first_clu: usize) {
        self.d_ino = first_clu;
        self.d_off = offset;
        self.fill_name(name);
    }

    fn fill_name(&mut self, name: &str) {
        let len = name.len().min(NAME_LIMIT);
        let name_bytes = name.as_bytes();
        for i in 0..len {
            self.d_name[i] = name_bytes[i];
        }
        self.d_name[len] = 0;
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Self>();
        unsafe { core::slice::from_raw_parts(self as *const _ as usize as *const u8, size) }
    }
}

