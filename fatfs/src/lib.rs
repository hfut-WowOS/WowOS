#![no_std]
extern crate alloc;

mod block_cache;
mod block_dev;
mod fat32_manager;
mod layout;
mod vfs;


pub const BLOCK_SIZE: usize = 512;
pub use block_dev::BlockDevice;
pub use layout::ShortDirEntry;
pub use vfs::{VFile,create_root_vfile};
use block_cache::{get_block_cache, set_start_sec, write_to_dev};
pub use fat32_manager::FAT32Manager;
pub use layout::*;

#[cfg(feature = "calc_hit_rate")]
pub use block_cache::{CACHEGET_NUM,CACHEHIT_NUM};

/// 用于将一个切片克隆为指定类型的数组。它接受一个切片和目标数组类型作为参数，并返回克隆后的数组。
pub fn clone_into_array<A, T>(slice: &[T]) -> A
where
    A: Default + AsMut<[T]>,
    T: Clone,
{
    let mut a = Default::default();
    <A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
    a
}
