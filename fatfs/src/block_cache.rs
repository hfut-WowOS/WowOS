use super::{BlockDevice, BLOCK_SZ};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
//读写锁
use spin::RwLock;

// BlockCache 结构体表示单个磁盘块的缓存。
// 它包含一个 cache 字段用于存储实际的块数据，
// 一个 block_id 字段用于标识块，
// 一个 block_device 字段表示底层块设备，
// 以及一个 modified 标志，指示缓存是否已被修改。
pub struct BlockCache {
    pub cache: [u8; BLOCK_SZ],
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
    modified: bool,
}

impl BlockCache {
    /// 从磁盘上加载一个块缓存
    pub fn new(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        let mut cache = [0u8; BLOCK_SZ];
        block_device.read_block(block_id, &mut cache);
        Self {
            cache,
            block_id,
            block_device,
            modified: false,
        }
    }

    /// 得到缓冲区中指定偏移量 offset 的字节地址
    fn addr_of_offset(&self, offset: usize) -> usize {
        &self.cache[offset] as *const _ as usize
    }

    /// 获取缓冲区中的位于偏移量 offset 的一个类型为 T 的磁盘上数据结构的不可变引用
    fn get_ref<T>(&self, offset: usize) -> &T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        let addr = self.addr_of_offset(offset);
        unsafe { &*(addr as *const T) }
    }

    /// 获取缓冲区中的位于偏移量 offset 的一个类型为 T 的磁盘上数据结构的可变引用
    fn get_mut<T>(&mut self, offset: usize) -> &mut T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        self.modified = true;
        let addr = self.addr_of_offset(offset);
        unsafe { &mut *(addr as *mut T) }
    }

    /// 获取不可变引用后执行指定函数
    pub fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    /// 获取可变引用后执行指定函数
    pub fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    /// 将缓冲区中的内容写回到磁盘块中
    fn sync(&mut self) {
        if self.modified {
            self.modified = false;
            self.block_device.write_block(self.block_id, &self.cache);
        }
    }
}

// Drop 特性的实现确保当 BlockCache 实例超出作用域时，缓存数据会写回底层块设备。
impl Drop for BlockCache {
    fn drop(&mut self) {
        self.sync()
    }
}

// BlockCacheManager 结构体表示多个块缓存的管理器。它维护一个具有指定限制的块缓存队列，并跟踪起始扇区。
// 双缓存：数据块和索引块，Clock算法进行淘汰
pub struct BlockCacheManager {
    start_sec: usize,
    limit: usize,
    queue: VecDeque<(usize, Arc<RwLock<BlockCache>>)>,
}

impl BlockCacheManager {
    pub fn new(limit: usize) -> Self {
        Self {
            start_sec: 0,
            limit,
            queue: VecDeque::new(),
        }
    }

    // 返回起始扇区
    pub fn get_start_sec(&self)->usize {
        self.start_sec
    }

    // 设置起始扇区的值。
    pub fn set_start_sec(&mut self, new_start_sec: usize) {
        self.start_sec = new_start_sec;
    }

    pub fn read_block_cache(
        &self,
        block_id: usize,
        //block_device: Arc<dyn BlockDevice>,
    ) -> Option<Arc<RwLock<BlockCache>>>{
        if let Some(pair) = self.queue
            .iter()
            .find(|pair| pair.0 == block_id) {
                Some(Arc::clone(&pair.1))
        }else{
            None
        }
    }

    // 获取一个块缓存
    // 以 block_id 和 Arc<dyn BlockDevice> 作为参数，并返回一个 Arc<RwLock<BlockCache>>，表示获取到的块缓存。
    pub fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<RwLock<BlockCache>> {
        if let Some(pair) = self.queue
            .iter()
            .find(|pair| pair.0 == block_id) {
                Arc::clone(&pair.1)
        } else {
            // substitute
            if self.queue.len() == self.limit/*BLOCK_CACHE_SIZE*/ {
                // from front to tail
                if let Some((idx, _)) = self.queue
                    .iter()
                    .enumerate()
                    .find(|(_, pair)| Arc::strong_count(&pair.1) == 1) {
                    self.queue.drain(idx..=idx);
                } else {
                    panic!("Run out of BlockCache!");
                }
            }
            // load block into mem and push back
            let block_cache = Arc::new(RwLock::new(
                BlockCache::new(block_id, Arc::clone(&block_device))
            ));
            self.queue.push_back((block_id, Arc::clone(&block_cache)));
            //println!("blkcache: {:?}", block_cache.read().cache);
            block_cache
        }
    }

    pub fn drop_all(&mut self) {
        self.queue.clear();
    }
}


// 64个缓存块，即 32KB
lazy_static! {
    pub static ref DATA_BLOCK_CACHE_MANAGER: RwLock<BlockCacheManager> = RwLock::new(
        BlockCacheManager::new(1034)
    );
}

lazy_static! {
    pub static ref INFO_CACHE_MANAGER: RwLock<BlockCacheManager> = RwLock::new(
        BlockCacheManager::new(10)
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
        if let Some(blk) = INFO_CACHE_MANAGER.read().read_block_cache(phy_blk_id){
            return blk
        }
        DATA_BLOCK_CACHE_MANAGER.write().get_block_cache(phy_blk_id, block_device);
        DATA_BLOCK_CACHE_MANAGER.read().read_block_cache(phy_blk_id).unwrap()
    } else {
        if let Some(blk) = INFO_CACHE_MANAGER.read().read_block_cache(phy_blk_id){
            return blk
        }
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
        if let Some(blk) = DATA_BLOCK_CACHE_MANAGER.read().read_block_cache(phy_blk_id){
            return blk
        }
        INFO_CACHE_MANAGER.write().get_block_cache(phy_blk_id, block_device);
        INFO_CACHE_MANAGER.read().read_block_cache(phy_blk_id).unwrap()
    } else {
        if let Some(blk) = DATA_BLOCK_CACHE_MANAGER.read().read_block_cache(phy_blk_id){
            return blk
        }
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