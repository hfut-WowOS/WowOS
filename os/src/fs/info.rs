use alloc::string::String;

const MAX_NAME_LENTH: usize = 255;

pub const DT_UNKNOWN: u8 = 0; //未知类型
pub const DT_FIFO: u8 = 1; //管道
pub const DT_DIR: u8 = 4; //目录
pub const DT_REG: u8 = 8; //常规文件
pub const DT_LNK: u8 = 10; //符号链接
pub const DT_WHT: u8 = 14; //链接

#[repr(C)]
#[derive(Debug)]
pub struct dirent {
    pub d_ino: u64,                   /* inode number 索引节点号 */
    pub d_off: i64,                   /* offset to this dirent 在目录文件中的偏移 */
    pub d_reclen: u16,                /* length of this d_name 文件名长 */
    pub d_type: u8,                   /* the type of d_name 文件类型 */
    pub d_name: [u8; MAX_NAME_LENTH], /* file name (null-terminated) 文件名，最长255字符 */
}

impl dirent {
    pub fn new(name: String, dinfo: u64, doff: i64, dtype: u8) -> Self {
        Self {
            d_ino: dinfo,
            d_off: doff,
            d_reclen: name.len() as u16,
            d_type: dtype,
            d_name: {
                let mut tmp: [u8; MAX_NAME_LENTH] = [0; MAX_NAME_LENTH];
                tmp[..name.len()].copy_from_slice(name.as_bytes());
                tmp
            },
        }
    }

    pub fn fill_info(&mut self, name: &str, inode: u64, d_off: i64, d_reclen: u16, d_type: u8) {
        *self = Self {
            d_ino: inode,
            d_off,
            d_reclen,
            d_type,
            d_name: Self::str2bytes(name),
        };
    }

    fn str2bytes(str: &str) -> [u8; MAX_NAME_LENTH] {
        let bytes = str.as_bytes();
        let len = bytes.len();
        assert!(len <= 128);
        let mut buf = [0u8; 128];
        let copy_part = &mut buf[..len];
        copy_part.copy_from_slice(bytes);
        buf
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Self>();
        unsafe { core::slice::from_raw_parts(self as *const _ as usize as *const u8, size) }
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
            st_blksize: 512 as u32,
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

// #[repr(C)]
// pub struct stat {
//     pub st_mode: u32,    //文件访问权限
//     pub st_info: u64,     //索引节点号
//     pub st_dev: u64,     //文件使用的设备号
//     pub st_rdev: u64,    //设备文件的设备号
//     pub st_nlink: u32,   //文件的硬连接数
//     pub st_uid: u32,     //所有者用户识别号
//     pub st_gid: u32,     //组识别号
//     pub st_size: i64,    //以字节为单位的文件容量
//     pub st_atime: i64,   //最后一次访问该文件的时间
//     pub st_mtime: i64,   //最后一次修改该文件的时间
//     pub st_ctime: i64,   //最后一次改变该文件状态的时间
//     pub st_blksize: u64, //包含该文件的磁盘块的大小
//     pub st_blocks: u64,  //该文件所占的磁盘块
// }

// impl stat {
//     pub fn new(
//         st_mode: u32,
//         st_info: u64,
//         st_dev: u64,
//         st_nlink: u32,
//         st_size: i64,
//         st_atime: i64,
//         st_mtime: i64,
//         st_ctime: i64,
//     ) -> Self {
//         Self {
//             st_mode,
//             st_info,
//             st_dev,
//             st_rdev: 0,
//             st_nlink,
//             st_uid: 0,
//             st_gid: 0,
//             st_size,
//             st_atime,
//             st_mtime,
//             st_ctime,
//             st_blksize: 0,
//             st_blocks: 0,
//         }
//     }

//     pub fn as_bytes(&self) -> &[u8] {
//         let size = core::mem::size_of::<Self>();
//         unsafe { core::slice::from_raw_parts(self as *const _ as usize as *const u8, size) }
//     }
// }
