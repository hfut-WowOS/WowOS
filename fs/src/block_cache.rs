use super::{BlockDevice, BLOCK_SZ};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use lazy_static::*;
use spin::{Mutex, RwLock};

/// 实现磁盘块缓存功能的块缓存层
pub struct BlockCache {
    cache: Vec<u8>,                         // 表示位于内存中的缓冲区
    block_id: usize,                        // 记录了这个块缓存来自于磁盘中的块的编号
    block_device: Arc<dyn BlockDevice>,     // 一个底层块设备的引用，可通过它进行块读写
    modified: bool,                         // 记录这个块从磁盘载入内存缓存之后，它有没有被修改过
    #[allow(unused)]
    time_stamp: usize,                      // TODO: 时间戳
}

impl BlockCache {
    /// Load a new BlockCache from disk.
    pub fn new(
        block_id: usize, 
        block_device: Arc<dyn BlockDevice>
    ) -> Self {
        let mut cache = vec![0u8; BLOCK_SZ];
        block_device.read_block(block_id, &mut cache);
        // TODO: 时间戳
        //let mut time_stamp = time::read();
        let time_stamp = 0;
        Self {
            cache,
            block_id,
            block_device,
            modified: false,
            time_stamp,
        }
    }

    /// 得到一个 BlockCache 内部的缓冲区中指定偏移量 offset 的字节地址
    fn addr_of_offset(&self, offset: usize) -> usize {
        &self.cache[offset] as *const _ as usize
    }

    /// 获取缓冲区中的位于偏移量 offset 的一个类型为 T 的磁盘上数据结构的不可变引用
    pub fn get_ref<T>(&self, offset: usize) -> &T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        let addr = self.addr_of_offset(offset);
        unsafe { &*(addr as *const T) }
    }

    /// 获取磁盘上数据结构的可变引用，由此可以对数据结构进行修改
    pub fn get_mut<T>(&mut self, offset: usize) -> &mut T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        self.modified = true;
        let addr = self.addr_of_offset(offset);
        unsafe { &mut *(addr as *mut T) }
    }

    // 在 BlockCache 缓冲区偏移量为 offset 的位置获取一个类型为 T 的磁盘上数据结构的不可变引用
    pub fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    // 在 BlockCache 缓冲区偏移量为 offset 的位置获取一个类型为 T 的磁盘上数据结构的可变引用
    pub fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    /// 被修改过的话会将缓冲区的内容写回磁盘
    pub fn sync(&mut self) {
        if self.modified {
            self.modified = false;
            self.block_device.write_block(self.block_id, &self.cache);
        }
    }
}

impl Drop for BlockCache {
    fn drop(&mut self) {
        self.sync()
    }
}

// 0-info扇区
// 1-2 FAT1
// 3-4 FAT2
// 5-7 DirEntry
// 8-19 DATA
const BLOCK_CACHE_SIZE: usize = 20;

pub struct BlockCacheManager {
    start_sec: usize,
    queue: VecDeque<(usize, Arc<RwLock<BlockCache>>)>,
}

impl BlockCacheManager {
    pub fn new() -> Self {
        Self {
            start_sec: 0,
            queue: VecDeque::new(),
        }
    }
    pub fn set_start_sec(&mut self, new_start_sec: usize){
        self.start_sec = new_start_sec;
    }

    pub fn get_start_sec(&self)->usize {
        self.start_sec
    }

    pub fn read_block_cache(
        &self,
        block_id: usize,
    ) -> Option<Arc<RwLock<BlockCache>>>{
        if let Some(pair) = self.queue
            .iter()
            .find(|pair| pair.0 == block_id) {
                Some(Arc::clone(&pair.1))
        }else{
            None
        }
    }


    /// get_block_cache 方法尝试从块缓存管理器中获取一个编号为 block_id 的块的块缓存
    /// 如果找不到，会从磁盘读取到内存中
    /// 有可能会发生缓存替换
    pub fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<RwLock<BlockCache>> {
        // 遍历整个队列试图找到一个编号相同的块缓存，如果找到了，会将块缓存管理器中保存的块缓存的引用复制一份并返回
        if let Some(pair) = self.queue.iter().find(|pair| pair.0 == block_id) {
            Arc::clone(&pair.1)
        } else {
            // 对应找不到的情况，此时必须将块从磁盘读入内存中的缓冲区
            // 判断管理器保存的块缓存数量是否已经达到了上限
            // 达到了上限需要执行缓存替换算法，丢掉某个块缓存并空出一个空位
            if self.queue.len() == BLOCK_CACHE_SIZE {
                // from front to tail
                if let Some((idx, _)) = self
                    .queue
                    .iter()
                    .enumerate()
                    .find(|(_, pair)| Arc::strong_count(&pair.1) == 1)
                {
                    self.queue.drain(idx..=idx);
                } else {
                    panic!("Run out of BlockCache!");
                }
            }
            // load block into mem and push back
            let block_cache = Arc::new(RwLock::new(BlockCache::new(
                block_id,
                Arc::clone(&block_device),
            )));
            self.queue.push_back((block_id, Arc::clone(&block_cache)));
            block_cache
        }
    }

    pub fn drop_all(&mut self){
        self.queue.clear();
    }

}

lazy_static! {
    pub static ref DATA_BLOCK_CACHE_MANAGER: RwLock<BlockCacheManager> = RwLock::new(
        BlockCacheManager::new()
    );
}

lazy_static! {
    pub static ref INFO_CACHE_MANAGER: RwLock<BlockCacheManager> = RwLock::new(
        BlockCacheManager::new()
    );
}

#[derive(PartialEq,Copy,Clone,Debug)]
pub enum CacheMode {
    READ,
    WRITE,
}

/* 仅用于访问文件数据块，不包括目录项 */
pub fn get_block_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
    rw_mode: CacheMode,
) -> Arc<RwLock<BlockCache>> {
    let phy_blk_id = DATA_BLOCK_CACHE_MANAGER.read().get_start_sec() + block_id;
    if rw_mode == CacheMode::READ {
        // make sure the blk is in cache
        DATA_BLOCK_CACHE_MANAGER.write().get_block_cache(phy_blk_id, block_device);
        DATA_BLOCK_CACHE_MANAGER.read().read_block_cache(phy_blk_id).unwrap()
    } else {
        DATA_BLOCK_CACHE_MANAGER.write().get_block_cache(phy_blk_id, block_device)
    }
}

/* 用于访问保留扇区，以及目录项 */
pub fn get_info_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
    rw_mode: CacheMode,
) -> Arc<RwLock<BlockCache>> {
    let phy_blk_id = INFO_CACHE_MANAGER.read().get_start_sec() + block_id;
    if rw_mode == CacheMode::READ {
        // make sure the blk is in cache
        INFO_CACHE_MANAGER.write().get_block_cache(phy_blk_id, block_device);
        INFO_CACHE_MANAGER.read().read_block_cache(phy_blk_id).unwrap()
    } else {
        INFO_CACHE_MANAGER.write().get_block_cache(phy_blk_id, block_device)
    }
}

pub fn set_start_sec(start_sec: usize){
    INFO_CACHE_MANAGER.write().set_start_sec(start_sec);
    DATA_BLOCK_CACHE_MANAGER.write().set_start_sec(start_sec);
}

pub fn write_to_dev(){  
    INFO_CACHE_MANAGER.write().drop_all();
    DATA_BLOCK_CACHE_MANAGER.write().drop_all();
}