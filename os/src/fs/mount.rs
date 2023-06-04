/// os/src/fs/mount.rs
/// 定义了一个用于管理挂载点的MountTable结构体和一个全局的挂载点表MNT_TABLE。
/// 提供了挂载点管理的功能，可以用于将特殊设备和文件系统类型与挂载目录关联起来，并进行挂载和卸载操作。
/// MNT_TABLE全局对象可用于在整个系统中共享挂载点表的状态。
/// 目前，mount 和 umount 方法对于已满的挂载点表或未找到匹配项的情况只返回固定的错误码。
/// to do:添加更详细的错误处理逻辑，例如返回自定义的错误类型或提供错误码和错误信息的组合。

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::*;
use spin::Mutex;

const MNT_MAXLEN: usize = 16;

pub struct MountTable {
    // <特殊设备、挂载目录和文件系统类型>
    mnt_list: Vec<(String, String, String)>, // special, dir, fstype
}

impl MountTable {
    /// 用于将一个新的挂载点添加到挂载点表中。它接受特殊设备、挂载目录、文件系统类型和标志作为参数，并将其添加到mnt_list中。
    /// 如果挂载点表已满，返回-1
    /// 如果挂载目录已存在于表中，返回0即不需要挂载
    /// 否则返回0表示挂载成功
    pub fn mount(&mut self, special: String, dir: String, fstype: String, _flag: u32) -> isize {
        if self.mnt_list.len() == MNT_MAXLEN {
            return -1;
        }
        if self.mnt_list.iter().find(|&(_, d, _)| *d == dir).is_some() {
            return 0;
        }
        self.mnt_list.push((special, dir, fstype));
        return 0;
    }

    /// 从挂载点表中卸载一个挂载点。它接受特殊设备和标志作为参数，并遍历mnt_list查找匹配的挂载点，
    /// 如果找到匹配项，则将其从表中移除并返回0，
    /// 否则返回-1表示卸载失败。
    pub fn umount(&mut self, special: String, _flags: u32) -> isize {
        let len = self.mnt_list.len();
        for i in 0..len {
            //println!("[umount] in mntlist = {}", self.mnt_list[i].0);
            if self.mnt_list[i].0 == special || self.mnt_list[i].1 == special {
                self.mnt_list.remove(i);
                return 0;
            }
        }
        return -1;
    }
}

// 创建全局的MNT_TABLE实例
lazy_static! {
    pub static ref MNT_TABLE: Arc<Mutex<MountTable>> = {
        let mnt_table = MountTable {
            mnt_list: Vec::new(),
        };
        Arc::new(Mutex::new(mnt_table))
    };
}
