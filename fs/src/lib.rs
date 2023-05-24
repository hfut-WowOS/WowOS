#![no_std]
extern crate alloc;

mod block_cache;
mod block_dev;
mod fat32_manager;
mod layout;
mod vfs;

pub const BLOCK_SZ: usize = 512;
pub const SECTOR_SIZE: usize = 8192;
pub const FAT_SIZE: usize = 400;
pub const DATA_SIZE: usize = 7390;

pub const FIRST_FAT_SEC: usize = 2;

use block_cache::{get_block_cache, get_info_cache, set_start_sec, write_to_dev, CacheMode};
pub use block_dev::BlockDevice;
pub use fat32_manager::FAT32Manager;
pub use layout::ShortDirEntry;
pub use layout::*;
pub use vfs::VFile;

pub fn clone_into_array<A, T>(slice: &[T]) -> A
where
    A: Default + AsMut<[T]>,
    T: Clone,
{
    let mut a = Default::default();
    <A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
    a
}
