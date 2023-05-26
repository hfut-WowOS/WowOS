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
    pub d_ino: u64,            /* inode number 索引节点号 */
    pub d_off: i64,            /* offset to this dirent 在目录文件中的偏移 */
    pub d_reclen: u16,         /* length of this d_name 文件名长 */
    pub d_type: u8,            /* the type of d_name 文件类型 */
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

    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Self>();
        unsafe { core::slice::from_raw_parts(self as *const _ as usize as *const u8, size) }
    }
}

#[repr(C)]
pub struct stat {
    pub st_mode: u32,    //文件访问权限
    pub st_info: u64,     //索引节点号
    pub st_dev: u64,     //文件使用的设备号
    pub st_rdev: u64,    //设备文件的设备号
    pub st_nlink: u32,   //文件的硬连接数
    pub st_uid: u32,     //所有者用户识别号
    pub st_gid: u32,     //组识别号
    pub st_size: i64,    //以字节为单位的文件容量
    pub st_atime: i64,   //最后一次访问该文件的时间
    pub st_mtime: i64,   //最后一次修改该文件的时间
    pub st_ctime: i64,   //最后一次改变该文件状态的时间
    pub st_blksize: u64, //包含该文件的磁盘块的大小
    pub st_blocks: u64,  //该文件所占的磁盘块
}

impl stat {
    pub fn new(
        st_mode: u32,
        st_info: u64,
        st_dev: u64,
        st_nlink: u32,
        st_size: i64,
        st_atime: i64,
        st_mtime: i64,
        st_ctime: i64,
    ) -> Self {
        Self {
            st_mode,
            st_info,
            st_dev,
            st_rdev: 0,
            st_nlink,
            st_uid: 0,
            st_gid: 0,
            st_size,
            st_atime,
            st_mtime,
            st_ctime,
            st_blksize: 0,
            st_blocks: 0,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        let size = core::mem::size_of::<Self>();
        unsafe { core::slice::from_raw_parts(self as *const _ as usize as *const u8, size) }
    }
}
