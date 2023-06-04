use super::{
    clone_into_array, fat32_manager::FAT32Manager, get_block_cache, BlockDevice, BLOCK_SIZE,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;

const LEAD_SIGNATURE: u32 = 0x41615252; // 引导扇区的标识符
const STRUC_SIGNATURE: u32 = 0x61417272; // 数据结构的标识符
pub const FREE_CLUSTER: u32 = 0x00000000; // 空闲簇
pub const END_CLUSTER: u32 = 0x0FFFFFF8; // 最后一个簇
pub const BAD_CLUSTER: u32 = 0x0FFFFFF7; // 表示坏簇的值

/* 表示文件或目录属性的常量 */
pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_VOLUME_ID: u8 = 0x08;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;
pub const ATTR_LONG_NAME: u8 = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_VOLUME_ID;

pub const DIRENT_SZ: usize = 32;
pub const LONG_NAME_LEN: u32 = 13;

type DataBlock = [u8; BLOCK_SIZE];

/// 用于表示FAT32文件系统的DBR（DOS引导记录）和BPB（BIOS参数块）。
#[repr(packed)]
#[derive(Clone, Copy, Debug)]
#[allow(unused)]
pub struct FatBS {
    bs_jmp_boot: [u8; 3],   // 跳转指令，指向启动代码
    bs_oem_name: [u8; 8],   // 建议值为“MSWIN4.1”
    bpb_bytes_per_sec: u16, // 每扇区的字节数
    bpb_sec_per_clus: u8,   // 每簇的扇区数
    bpb_rsvd_sec_cnt: u16,  // 保留扇区的数目，通过它能获得第一个FAT表所在的扇区
    bpb_num_fats: u8,       // FAT数
    bpb_root_ent_cnt: u16, // 对于FAT12和FAT16此域包含根目录中目录的个数（每项长度为32字节），对于FAT32，此项必须为0。
    bpb_tot_sec16: u16,    // 早期版本中16bit的总扇区，对于FAT32，此域必为0。
    bpb_media: u8,         // 媒体描述符
    bpb_fatsz16: u16,      // FAT12/FAT16一个FAT表所占的扇区数，对于FAT32来说此域必须为0
    bpb_sec_per_trk: u16,  // 每磁道的扇区数，用于BIOS中断0x13
    bpb_num_heads: u16,    // 磁头数，用于BIOS的0x13中断
    bpb_hidd_sec: u32, // 在此FAT分区之前所隐藏的扇区数，必须使得调用BIOS的0x13中断可以得到此数值，对于那些没有分区的存储介质，此域必须为0
    bpb_tot_sec32: u32, // 该卷总扇区数（32bit），这里的扇区总数包括四个基本部分，此域可以为0，若此域为0，BPB_ToSec16必须为非0，对FAT32，此域必须是非0。
}

impl FatBS {
    /// 返回每扇区的字节数。
    pub fn bytes_per_sec(&self) -> u32 {
        self.bpb_bytes_per_sec as u32
    }

    /// 返回每簇的扇区数。
    pub fn sec_per_clus(&self) -> u32 {
        self.bpb_sec_per_clus as u32
    }

    /// 返回FAT表的数量。
    pub fn fat_num(&self) -> u32 {
        self.bpb_num_fats as u32
    }

    /// 返回保留扇区的数量。
    pub fn rsvd_sec_cnt(&self) -> u32 {
        self.bpb_rsvd_sec_cnt as u32
    }
}

/// FAT32 Structure Starting at Offset 36(0x24)
#[repr(packed)]
#[derive(Clone, Copy)]
#[allow(unused)]
pub struct FatExtBS {
    bpb_fatsz32: u32,   // 一个FAT表所占的扇区数，此域为FAT32特有，同时BPB_FATSz16必须为0
    bpb_ext_flags: u16, // 扩展标志，此域FAT32特有
    bpb_fs_ver: u16,    // 此域为FAT32特有， 高位为FAT32的主版本号，低位为次版本号
    bpb_root_clus: u32, // 根目录所在第一个簇的簇号，通常该数值为2，但不是必须为2。
    bpb_fsinfo: u16,    // 保留区中FAT32卷FSINFO结构所在的扇区号，通常为1。
    bpb_bk_boot_sec: u16, // 如果不为0，表示在保留区中引导记录的备数据所占的扇区数，通常为6。
    bpb_reserved: [u8; 12], // 用于以后FAT扩展使用，对FAT32。此域用0填充
    bs_drv_num: u8,     // 用于BIOS中断0x13得到磁盘驱动器参数
    bs_reserved1: u8,   // 保留（供NT使用），格式化FAT卷时必须设为0
    bs_boot_sig: u8,    // 扩展引导标记（0x29）用于指明此后的3个域可用
    bs_vol_id: u32,     // 卷标序列号，此域以BS_VolLab一起可以用来检测磁盘是否正确
    bs_vol_lab: [u8; 11], // 磁盘卷标，此域必须与根目录中11字节长的卷标一致。
    bs_fil_sys_type: [u8; 64], // 以下的几种之一：“FAT12”，“FAT16”，“FAT32”。
}

impl FatExtBS {
    /// FAT占用的扇区数
    pub fn fat_size(&self) -> u32 {
        self.bpb_fatsz32
    }

    /// 获取 FSInfo 所在的扇区号
    pub fn fat_info_sec(&self) -> u32 {
        self.bpb_fsinfo as u32
    }
}

#[repr(packed)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub struct FSInfo {
    fsi_lead_sig: u32,        // Value 0x41615252
    fsi_reserved1: [u8; 480], // 保留
    fsi_struc_sig: u32,       // Value 0x61417272
    fsi_free_count: u32, // 包含卷上最近已知的空闲簇计数。如果值是0xFFFFFFFF，那么空闲计数是未知的，必须计算。
    fsi_nxt_free: u32,   // 最后被分配的簇号，起始空闲簇号应该是下一个簇
    fsi_reserved2: [u8; 12], // 保留
    fsi_trail_sig: u32,  // Trail signature (0xAA550000)
}

impl FSInfo {
    /// 对签名进行校验
    pub fn check_signature(&self) -> bool {
        self.fsi_lead_sig == LEAD_SIGNATURE && self.fsi_struc_sig == STRUC_SIGNATURE
    }

    /// 读取空闲簇数
    pub fn free_clusters(&self) -> u32 {
        self.fsi_free_count
    }

    /// 写入空闲簇数
    pub fn set_free_clusters(&mut self, free_clusters: u32) {
        self.fsi_free_count = free_clusters
    }

    /// 读取最后被分配的簇号
    pub fn next_free_cluster(&self) -> u32 {
        self.fsi_nxt_free
    }

    /// 写入最后被分配的簇号
    pub fn set_next_free_cluster(&mut self, start_cluster: u32) {
        self.fsi_nxt_free = start_cluster
    }
}


// 在FAT32文件系统中，短目录项（Short Directory Entry）是一种用于表示文件和目录的数据结构。
// 每个短目录项占用32个字节，用于存储文件的基本信息。
// FAT 32 Byte Directory Entry Structure
// 11+1+1+1+2+2+2+2+2+2+2+4 = 32
#[derive(Clone, Copy, Debug)]
#[repr(packed)]
#[allow(unused)]
pub struct ShortDirEntry {
    dir_name: [u8; 8],      // 短文件名
    dir_extension: [u8; 3], // 扩展名
    dir_attr: u8,           // 文件属性
    dir_ntres: u8,          // 保留给 Windows NT 使用
    dir_crt_time_tenth: u8, // 文件创建的时间戳
    dir_crt_time: u16,      // 文件创建的时间
    dir_crt_date: u16,      // 文件创建的日期
    dir_lst_acc_date: u16,  // 上一次访问日期
    dir_fst_clus_hi: u16,   // 文件起始簇号的高 16位
    dir_wrt_time: u16,      // 上一次写入的时间
    dir_wrt_date: u16,      // 上一次写入的日期
    dir_fst_clus_lo: u16,   // 文件起始簇号的低 16位
    dir_file_size: u32,     // 文件大小（以字节为单位）
}
// 在FAT32文件系统中，目录被组织为一个包含多个短目录项的表格，每个目录项占用固定大小的空间。
// 通过遍历目录项表格，可以获取目录中的所有文件和子目录的信息。

impl ShortDirEntry {
    /// 建一个空的目录项
    pub fn new() -> Self {
        Self {
            dir_name: [0; 8],
            dir_extension: [0; 3],
            dir_attr: 0,
            dir_ntres: 0,
            dir_crt_time_tenth: 0,
            dir_crt_time: 0,
            dir_crt_date: 0,
            dir_lst_acc_date: 0,
            dir_fst_clus_hi: 0,
            dir_wrt_time: 0,
            dir_wrt_date: 0,
            dir_fst_clus_lo: 0,
            dir_file_size: 0,
        }
    }

    /// 初始化，这里仅修改名字与属性
    pub fn initialize(&mut self, name_: &[u8], extension_: &[u8], dir_attr: u8) {
        self.dir_name = clone_into_array(&name_[0..8]);
        self.dir_extension = clone_into_array(&extension_[0..3]);
        self.dir_attr = dir_attr;
    }

    pub fn is_valid(&self) -> bool {
        // 目前未删除即有效
        if self.dir_name[0] != 0xE5 {
            true
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        if self.dir_name[0] == 0x00 {
            true
        } else {
            false
        }
    }

    pub fn is_dir(&self) -> bool {
        if 0 != (self.dir_attr & ATTR_DIRECTORY) {
            true
        } else {
            false
        }
    }

    pub fn is_long(&self) -> bool {
        if self.dir_attr == ATTR_LONG_NAME {
            true
        } else {
            false
        }
    }

    pub fn attr(&self) -> u8 {
        self.dir_attr
    }

    pub fn file_size(&self) -> u32 {
        self.dir_file_size
    }

    /// 设置当前文件的大小
    pub fn set_file_size(&mut self, dir_file_size: u32) {
        self.dir_file_size = dir_file_size;
    }

    /// 获取文件起始簇号
    pub fn first_cluster(&self) -> u32 {
        ((self.dir_fst_clus_hi as u32) << 16) + (self.dir_fst_clus_lo as u32)
    }

    /// 设置文件起始簇
    pub fn set_first_cluster(&mut self, cluster: u32) {
        self.dir_fst_clus_hi = ((cluster & 0xFFFF0000) >> 16) as u16; // 设置高位
        self.dir_fst_clus_lo = (cluster & 0x0000FFFF) as u16; // 设置低位
    }

    /// 获取短文件名(大写)
    pub fn get_name_uppercase(&self) -> String {
        let mut name: String = String::new();
        for i in 0..8 {
            // 记录文件名
            if self.dir_name[i] == 0x20 {
                break;
            } else {
                name.push(self.dir_name[i] as char);
            }
        }
        for i in 0..3 {
            // 记录扩展名
            if self.dir_extension[i] == 0x20 {
                break;
            } else {
                if i == 0 {
                    name.push('.');
                }
                name.push(self.dir_extension[i] as char);
            }
        }
        name
    }

    #[allow(unused_assignments)]
    /// 获取短文件名(小写)
    pub fn get_name_lowercase(&self) -> String {
        let mut name: String = String::new();
        name = self.get_name_uppercase().to_ascii_lowercase();
        name
    }

    pub fn set_case(&mut self, case: u8) {
        self.dir_ntres = case;
    }

    // 决赛要求64位的时间戳，但 fat32 貌似不是这样，这边先跳过，有空再来完善
    pub fn time(&self) -> u64 {
        self.dir_wrt_time as u64
    }

    // 决赛要求64位的时间戳，但 fat32 貌似不是这样，这边先跳过，有空再来完善
    pub fn set_time(&mut self, tv_sec: u64, tv_nsec: u64) {
        _ = tv_sec;
        _ = tv_nsec;
        self.dir_wrt_time = 12345 as u16;
    }

    /// 清空文件，删除时使用
    pub fn clear(&mut self) {
        self.dir_file_size = 0;
        self.set_first_cluster(0);
    }

    pub fn delete(&mut self) {
        self.clear();
        self.dir_name[0] = 0xE5;
    }

    /// 获取文件偏移量所在的簇、扇区和偏移
    pub fn get_pos(
        &self,
        offset: usize,
        manager: &Arc<RwLock<FAT32Manager>>,
        fat: &Arc<RwLock<FAT>>,
        block_device: &Arc<dyn BlockDevice>,
    ) -> (u32, usize, usize) {
        let manager_reader = manager.read();
        let fat_reader = fat.read();
        let bytes_per_sector = manager_reader.bytes_per_sector() as usize;
        let bytes_per_cluster = manager_reader.bytes_per_cluster() as usize;
        let cluster_index = offset / bytes_per_cluster;
        let current_cluster = fat_reader.get_cluster_at(
            self.first_cluster(),
            cluster_index as u32,
            Arc::clone(block_device),
        );
        let current_sector = manager_reader.first_sector_of_cluster(current_cluster)
            + (offset - cluster_index * bytes_per_cluster) / bytes_per_sector;
        (current_cluster, current_sector, offset % bytes_per_sector)
    }

    pub fn read_at(
        &self,
        offset: usize,
        buf: &mut [u8],
        manager: &Arc<RwLock<FAT32Manager>>,
        fat: &Arc<RwLock<FAT>>,
        block_device: &Arc<dyn BlockDevice>,
    ) -> usize {
        // println!("enter read_at: offset:{}",offset);
        let manager_reader = manager.read();
        let fat_reader = fat.read();
        let bytes_per_sector = manager_reader.bytes_per_sector() as usize;
        let bytes_per_cluster = manager_reader.bytes_per_cluster() as usize;
        let sectors_per_cluster = manager_reader.sectors_per_cluster() as usize;

        let mut offset = offset;
        let mut current_cluster = self.first_cluster();
        if current_cluster == 0 || buf.len() == 0 {
            return 0;
        }

        let mut read_size = 0usize;
        let mut rest = buf.len();

        while current_cluster < END_CLUSTER && rest > 0 {
            if offset >= bytes_per_cluster {
                offset -= bytes_per_cluster;
                current_cluster =
                    fat_reader.get_next_cluster(current_cluster, Arc::clone(block_device));
                continue;
            }
            // println!("current_cluster:0x{:x},rest:{}", current_cluster,rest);
            let nth_sect = offset / bytes_per_sector;
            let sect = manager_reader.first_sector_of_cluster(current_cluster);
            offset %= bytes_per_sector;
            for i in nth_sect..sectors_per_cluster {
                if rest <= 0 {
                    break;
                }
                let len = rest.min(bytes_per_sector - offset);
                // println!("1:rest:{}, len:{}",rest,len);
                get_block_cache((sect + i) as usize, Arc::clone(block_device))
                    .read()
                    .read(0, |data_block: &DataBlock| {
                        // println!("offset:{},len:{}",offset,len);
                        let src = &data_block[offset..offset + len];
                        let dst = &mut buf[read_size..read_size + len];
                        dst.copy_from_slice(src);
                    });
                rest -= len;
                read_size += len;
                offset = 0;
            }
            if rest == 0 {
                break;
            }
            current_cluster =
                fat_reader.get_next_cluster(current_cluster, Arc::clone(block_device));
        }
        read_size
    }

    pub fn write_at(
        &self,
        offset: usize,
        buf: &[u8],
        manager: &Arc<RwLock<FAT32Manager>>,
        fat: &Arc<RwLock<FAT>>,
        block_device: &Arc<dyn BlockDevice>,
    ) -> usize {
        let manager_reader = manager.read();
        let fat_reader = fat.read();
        let bytes_per_sector = manager_reader.bytes_per_sector() as usize;
        let bytes_per_cluster = manager_reader.bytes_per_cluster() as usize;
        let sectors_per_cluster = manager_reader.sectors_per_cluster() as usize;

        let mut offset = offset;
        let mut current_cluster = self.first_cluster();

        if current_cluster == 0 || buf.len() == 0 {
            return 0;
        }

        let mut write_size = 0usize;
        let mut rest = buf.len();

        while current_cluster < END_CLUSTER && rest > 0 {
            if offset >= bytes_per_cluster {
                offset -= bytes_per_cluster;
                current_cluster =
                    fat_reader.get_next_cluster(current_cluster, Arc::clone(block_device));
                continue;
            }
            let nth_sect = offset / bytes_per_sector;
            let sect = manager_reader.first_sector_of_cluster(current_cluster);
            offset %= bytes_per_sector;
            for i in nth_sect..sectors_per_cluster {
                if rest <= 0 {
                    break;
                }
                let len = rest.min(bytes_per_sector - offset);
                get_block_cache((sect + i) as usize, Arc::clone(block_device))
                    .write()
                    .modify(0, |data_block: &mut DataBlock| {
                        let src = &buf[write_size..write_size + len];
                        let dst = &mut data_block[offset..offset + len];
                        dst.copy_from_slice(src);
                    });
                rest -= len;
                write_size += len;
                offset = 0
            }
            if rest == 0 {
                break;
            }
            current_cluster =
                fat_reader.get_next_cluster(current_cluster, Arc::clone(block_device));
        }
        write_size
    }

    /// 为相应的长文件名计算校验和
    pub fn checksum(&self) -> u8 {
        let mut name_buff: [u8; 11] = [0u8; 11];
        let mut sum: u8 = 0;
        for i in 0..8 {
            name_buff[i] = self.dir_name[i];
        }
        for i in 0..3 {
            name_buff[i + 8] = self.dir_extension[i];
        }
        for i in 0..11 {
            if (sum & 1) != 0 {
                sum = 0x80 + (sum >> 1) + name_buff[i];
            } else {
                sum = (sum >> 1) + name_buff[i];
            }
        }
        sum
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self as *mut _ as usize as *mut u8, DIRENT_SZ) }
    }
}

// 长目录项（Long Directory Entry）是一种在FAT文件系统中使用的特殊目录项类型。
// 与短目录项相比，长目录项用于存储更长的文件名，并提供更多的元数据信息。
// 在FAT文件系统中，短目录项仅能存储8个字符的文件名和3个字符的扩展名，而长目录项可以用于存储长达255个字符的文件名和扩展名。
// 这使得长目录项能够处理包含特殊字符、多语言字符或非标准字符的文件名。
// 长目录项由多个连续的32字节项组成，每个长目录项存储文件名的一部分。
// 文件名的最后一部分存储在短目录项中，这样可以通过将所有的长目录项和短目录项组合在一起来重建完整的文件名。
// 长目录项的引入主要是为了解决FAT文件系统中对文件名长度的限制，并提供更好的兼容性和国际化支持。
// FAT Long Directory Entry Structure
// 1+10+1+1+1+12+2+4 = 32
#[repr(packed)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub struct LongDirEntry {
    // 使用 Unicode 编码，即每个字符占用2个字节，一组 13 个字符，共 26 字节，即 10+12+4
    // 如果是该文件的最后一个长文件名目录项，
    // 则将该目录项的序号与 0x40 进行“或（OR）运算”的结果写入该位置。
    // 长文件名要有\0
    ldir_ord: u8, // 长文件名目录项的序列号，一个文件的第一个目录项序列号为 1，然后依次递增
    ldir_name1: [u8; 10], // 5 characters
    ldir_attr: u8, // 长文件名目录项标志，取值 0x0F
    ldir_type: u8, // 如果为零，则表示目录项是长文件名的一部分
    ldir_chksum: u8, // 根据对应短文件名计算出的校验值，用于长文件名与短文件名的匹配
    ldir_name2: [u8; 12], // 6 characters
    ldir_fst_clus_lo: [u8; 2], // 文件起始簇号，目前置 0
    ldir_name3: [u8; 4], // 2 characters
}

impl From<&[u8]> for LongDirEntry {
    fn from(bytes: &[u8]) -> Self {
        Self {
            ldir_ord: bytes[0],
            ldir_name1: clone_into_array(&bytes[1..11]),
            ldir_attr: bytes[11],
            ldir_type: bytes[12],
            ldir_chksum: bytes[13],
            ldir_name2: clone_into_array(&bytes[14..26]),
            ldir_fst_clus_lo: clone_into_array(&bytes[26..28]),
            ldir_name3: clone_into_array(&bytes[28..32]),
        }
    }
}

impl LongDirEntry {
    pub fn new() -> Self {
        Self {
            ldir_ord: 0,
            ldir_name1: [0; 10],
            ldir_attr: 0,
            ldir_type: 0,
            ldir_chksum: 0,
            ldir_name2: [0; 12],
            ldir_fst_clus_lo: [0; 2],
            ldir_name3: [0; 4],
        }
    }

    pub fn attr(&self) -> u8 {
        self.ldir_attr
    }

    pub fn order(&self) -> u8 {
        self.ldir_ord
    }
    pub fn check_sum(&self) -> u8 {
        self.ldir_chksum
    }

    pub fn is_empty(&self) -> bool {
        if self.ldir_ord == 0x00 {
            true
        } else {
            false
        }
    }

    pub fn is_valid(&self) -> bool {
        if self.ldir_ord == 0xE5 {
            false
        } else {
            true
        }
    }

    pub fn is_deleted(&self) -> bool {
        !self.is_valid()
    }

    /// 长文件名目录项初始化
    /// 传入长度为 13 的字符数组，暂不支持中文
    pub fn initialize(&mut self, name_buffer: &[u8], ldir_ord: u8, ldir_chksum: u8) {
        let mut ldir_name1: [u8; 10] = [0; 10];
        let mut ldir_name2: [u8; 12] = [0; 12];
        let mut ldir_name3: [u8; 4] = [0; 4];
        let mut end_offset = 0;
        for i in 0..5 {
            if end_offset == 0 {
                ldir_name1[i << 1] = name_buffer[i];
                if name_buffer[i] == 0 {
                    end_offset = i;
                }
            } else {
                ldir_name1[i << 1] = 0xFF;
                ldir_name1[(i << 1) + 1] = 0xFF;
            }
        }
        for i in 5..11 {
            if end_offset == 0 {
                ldir_name2[(i - 5) << 1] = name_buffer[i];
                if name_buffer[i] == 0 {
                    end_offset = i;
                }
            } else {
                ldir_name2[(i - 5) << 1] = 0xFF;
                ldir_name2[((i - 5) << 1) + 1] = 0xFF;
            }
        }
        for i in 11..13 {
            if end_offset == 0 {
                ldir_name3[(i - 11) << 1] = name_buffer[i];
                if name_buffer[i] == 0 {
                    end_offset = i;
                }
            } else {
                ldir_name3[(i - 11) << 1] = 0xFF;
                ldir_name3[((i - 11) << 1) + 1] = 0xFF;
            }
        }
        *self = Self {
            ldir_ord,
            ldir_name1,
            ldir_attr: ATTR_LONG_NAME,
            ldir_type: 0,
            ldir_chksum,
            ldir_name2,
            ldir_fst_clus_lo: [0u8; 2],
            ldir_name3,
        }
    }

    pub fn clear(&mut self) {
        //self.LDIR_Ord = 0xE5;
    }

    pub fn delete(&mut self) {
        self.ldir_ord = 0xE5;
    }

    /* 获取长文件名，此处完成unicode至ascii的转换，暂不支持中文！*/
    // 这里直接将几个字段拼合，不考虑填充字符0xFF等
    pub fn get_name_raw(&self) -> String {
        let mut name = String::new();
        let mut c: u8;
        for i in 0..5 {
            c = self.ldir_name1[i << 1];
            name.push(c as char);
        }
        for i in 0..6 {
            c = self.ldir_name2[i << 1];
            name.push(c as char);
        }
        for i in 0..2 {
            c = self.ldir_name3[i << 1];
            name.push(c as char);
        }
        return name;
    }

    pub fn get_name_format(&self) -> String {
        let mut name = String::new();
        let mut c: u8;
        for i in 0..5 {
            c = self.ldir_name1[i << 1];
            if c == 0 {
                return name;
            }
            name.push(c as char);
        }
        for i in 0..6 {
            c = self.ldir_name2[i << 1];
            if c == 0 {
                return name;
            }
            name.push(c as char);
        }
        for i in 0..2 {
            c = self.ldir_name3[i << 1];
            if c == 0 {
                return name;
            }
            name.push(c as char);
        }
        return name;
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self as *mut _ as usize as *mut u8, DIRENT_SZ) }
    }
}

// 一个FAT（File Allocation Table）的实现，用于处理FAT文件系统中的簇分配和簇链的操作
// 常驻内存，不作一一映射
// FAT文件系统使用FAT表来跟踪文件和目录的簇分配情况
#[derive(Clone, Copy, Debug)]
pub struct FAT {
    fat1_sector: u32, // FAT1的起始扇区
    fat2_sector: u32, // FAT2的起始扇区
}

impl FAT {
    pub fn new(fat1_sector: u32, fat2_sector: u32) -> Self {
        Self {
            fat1_sector,
            fat2_sector,
        }
    }

    // true
    /// 计算簇号对应表项所在的扇区和扇区内偏移
    fn calculate_pos(&self, cluster: u32) -> (u32, u32, u32) {
        // 返回 sector号和 offset
        // 前为FAT1的扇区号，后为FAT2的扇区号，最后为offset
        let fat1_sec = self.fat1_sector + (cluster * 4) / 512;
        let fat2_sec = self.fat2_sector + (cluster * 4) / 512;
        let offset = (4 * cluster) % 512;
        (fat1_sec, fat2_sec, offset)
    }

    /// 搜索下一个可用簇，仅在 FAT32Manager::alloc_cluster 中使用
    /// alloc_cluster 保证了可以找到空闲的簇
    /// 它从当前簇的下一个簇开始，依次查找FAT表项，直到找到一个空闲的簇。这个方法在分配新的簇时使用。
    pub fn get_free_cluster(
        &self,
        current_cluster: u32,
        block_device: Arc<dyn BlockDevice>,
    ) -> u32 {
        // 跳过当前簇
        let mut curr_cluster = current_cluster + 1;
        // 寻找空闲的簇，因为簇号分配是离散的而不是连续的，因此不能保证最后一个被分配的簇的下一个簇就是空闲的
        loop {
            let (fat1_sec, _, offset) = self.calculate_pos(curr_cluster);
            // 查看当前簇的表项
            let entry_val = get_block_cache(fat1_sec as usize, block_device.clone())
                .read()
                .read(offset as usize, |&entry_val: &u32| entry_val);
            if entry_val == FREE_CLUSTER {
                break;
            } else {
                curr_cluster += 1;
            }
        }
        // A FAT32 FAT entry is actually only a 28-bit entry. The high 4 bits of a FAT32 FAT entry are reserved.
        curr_cluster & 0x0FFFFFFF
    }

    /// 用于查询给定簇号在FAT表中的下一个簇号。
    /// 它根据给定的簇号计算对应的FAT表项的位置，然后从FAT表中读取该位置的值。
    /// 如果值表示簇号无效或未使用，则返回0。
    /// 如果簇号有效，则返回下一个簇号。
    pub fn get_next_cluster(&self, cluster: u32, block_device: Arc<dyn BlockDevice>) -> u32 {
        // 需要对损坏簇作出判断
        // 及时使用备用表
        // 无效或未使用返回0
        let (fat1_sec, fat2_sec, offset) = self.calculate_pos(cluster);

        let fat1_rs = get_block_cache(fat1_sec as usize, block_device.clone())
            .read()
            .read(offset as usize, |&next_cluster: &u32| next_cluster);
        let fat2_rs = get_block_cache(fat2_sec as usize, block_device.clone())
            .read()
            .read(offset as usize, |&next_cluster: &u32| next_cluster);

        if fat1_rs == BAD_CLUSTER {
            if fat2_rs == BAD_CLUSTER {
                0
            } else {
                fat2_rs & 0x0FFFFFFF
            }
        } else {
            fat1_rs & 0x0FFFFFFF
        }
    }

    /// 用于设置给定簇号在FAT表中的下一个簇号。
    /// 它根据给定的簇号计算对应的FAT表项的位置，并在FAT表中修改该位置的值为下一个簇号。
    pub fn set_next_cluster(
        &self,
        cluster: u32,
        next_cluster: u32,
        block_device: Arc<dyn BlockDevice>,
    ) {
        // 同步修改两个FAT
        let (fat1_sec, fat2_sec, offset) = self.calculate_pos(cluster);
        get_block_cache(fat1_sec as usize, block_device.clone())
            .write()
            .modify(offset as usize, |old_clu: &mut u32| {
                *old_clu = next_cluster;
            });
        get_block_cache(fat2_sec as usize, block_device.clone())
            .write()
            .modify(offset as usize, |old_clu: &mut u32| {
                *old_clu = next_cluster;
            });
    }

    /// 用于获取某个簇链中指定索引的簇号。
    /// 它从给定的起始簇开始，依次获取下一个簇号，直到达到指定的索引位置。
    /// 如果簇链结束或索引超过簇链长度，则返回0。
    pub fn get_cluster_at(
        &self,
        start_cluster: u32,
        index: u32,
        block_device: Arc<dyn BlockDevice>,
    ) -> u32 {
        let mut cluster = start_cluster;
        for _ in 0..index {
            cluster = self.get_next_cluster(cluster, block_device.clone());
            if cluster == 0 {
                break;
            }
        }
        cluster & 0x0FFFFFFF
    }

    /// 用于获取某个簇链的最后一个簇号。
    /// 它从给定的起始簇开始，依次获取下一个簇号，直到遇到结束标记或无效簇号。
    /// 返回最后一个有效的簇号。
    pub fn final_cluster(&self, start_cluster: u32, block_device: Arc<dyn BlockDevice>) -> u32 {
        let mut curr_cluster = start_cluster;
        assert_ne!(start_cluster, 0);
        loop {
            let next_cluster = self.get_next_cluster(curr_cluster, block_device.clone());
            if next_cluster >= END_CLUSTER || next_cluster == 0 {
                return curr_cluster & 0x0FFFFFFF;
            } else {
                curr_cluster = next_cluster;
            }
        }
    }

    /// 用于获取某个簇链从指定簇开始的所有簇号。
    /// 它从给定的起始簇开始，依次获取下一个簇号，将每个簇号添加到一个向量中，直到遇到结束标记或无效簇号。
    /// 返回包含所有簇号的向量。
    pub fn get_all_cluster_of(
        &self,
        start_cluster: u32,
        block_device: Arc<dyn BlockDevice>,
    ) -> Vec<u32> {
        let mut curr_cluster = start_cluster;
        let mut v_cluster: Vec<u32> = Vec::new();
        loop {
            v_cluster.push(curr_cluster & 0x0FFFFFFF);
            let next_cluster = self.get_next_cluster(curr_cluster, block_device.clone());
            if next_cluster >= END_CLUSTER || next_cluster == 0 {
                return v_cluster;
            } else {
                curr_cluster = next_cluster;
            }
        }
    }

    /// 用于统计某个簇链从指定簇开始到结尾的簇数。
    /// 它从给定的起始簇开始，依次获取下一个簇号，直到遇到结束标记或超出有效簇号范围。
    /// 返回簇的数量。
    pub fn count_claster_num(&self, start_cluster: u32, block_device: Arc<dyn BlockDevice>) -> u32 {
        if start_cluster == 0 {
            return 0;
        }
        let mut curr_cluster = start_cluster;
        let mut count: u32 = 0;
        loop {
            count += 1;
            let next_cluster = self.get_next_cluster(curr_cluster, block_device.clone());
            if next_cluster >= END_CLUSTER || next_cluster > 0xF000000 {
                return count;
            } else {
                curr_cluster = next_cluster;
            }
        }
    }
}
