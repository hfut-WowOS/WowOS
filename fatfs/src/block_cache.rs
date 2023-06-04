use super::{BlockDevice, BLOCK_SIZE};
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
    pub cache: [u8; BLOCK_SIZE],
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
    modified: bool,
}

impl BlockCache {
    /// 从磁盘上加载一个块缓存
    pub fn new(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        let mut cache = [0u8; BLOCK_SIZE];
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
        assert!(offset + type_size <= BLOCK_SIZE);
        let addr = self.addr_of_offset(offset);
        unsafe { &*(addr as *const T) }
    }

    /// 获取缓冲区中的位于偏移量 offset 的一个类型为 T 的磁盘上数据结构的可变引用
    fn get_mut<T>(&mut self, offset: usize) -> &mut T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SIZE);
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
    pub fn start_sec(&self) -> usize {
        self.start_sec
    }

    // 设置起始扇区的值。
    pub fn set_start_sec(&mut self, new_start_sec: usize) {
        self.start_sec = new_start_sec;
    }

    // 获取一个块缓存
    // 以 block_id 和 Arc<dyn BlockDevice> 作为参数，并返回一个 Arc<RwLock<BlockCache>>，表示获取到的块缓存。
    pub fn get_block_cache(&mut self, block_id: usize, block_device: Arc<dyn BlockDevice>) -> Arc<RwLock<BlockCache>> {
        // 先在队列中寻找，若找到则将块缓存的引用复制一份并返回
        if let Some(pair) = self.queue.iter().find(|pair| pair.0 == block_id) {
            #[cfg(feature = "calc_hit_rate")]
            unsafe {
                CACHEHIT_NUM = CACHEHIT_NUM + 1.0;
            }
            Arc::clone(&pair.1)
        } else {
            // 判断块缓存数量是否到达上线
            if self.queue.len() == self.limit {
                // FIFO 替换，找强引用计数为1的替换出去
                if let Some((idx, _)) = self.queue.iter().enumerate().find(|(_, pair)| Arc::strong_count(&pair.1) == 1) {
                    self.queue.drain(idx..=idx);
                } else {
                    // 队列已满且其中所有的块缓存都正在使用的情形
                    panic!("Run out of BlockCache!");
                }
            }
            // 创建新的块缓存（会触发 read_block 进行块读取）
            let block_cache = Arc::new(RwLock::new(BlockCache::new(block_id, Arc::clone(&block_device))));
            // 加入到队尾，最后返回
            self.queue.push_back((block_id, Arc::clone(&block_cache)));
            block_cache
        }
    }

    pub fn drop_all(&mut self) {
        self.queue.clear();
    }
}

// 64个缓存块，即 32KB
lazy_static! {
    pub static ref BLOCK_CACHE_MANAGER: RwLock<BlockCacheManager> = RwLock::new(BlockCacheManager::new(64));
}

// 定义了两个静态的可变变量 CACHEGET_NUM 和 CACHEHIT_NUM，用于计算缓存的命中率。
#[cfg(feature = "calc_hit_rate")]
pub static mut CACHEGET_NUM: f64 = 0.0;
#[cfg(feature = "calc_hit_rate")]
pub static mut CACHEHIT_NUM: f64 = 0.0;

/// 用于外部模块访问文件数据块
pub fn get_block_cache(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Arc<RwLock<BlockCache>> {
    // 这里的read是RWLock读写锁
    #[cfg(feature = "calc_hit_rate")]
    unsafe {
        CACHEGET_NUM = CACHEGET_NUM + 1.0;
    }
    let phy_blk_id = BLOCK_CACHE_MANAGER.read().start_sec() + block_id;
    BLOCK_CACHE_MANAGER.write().get_block_cache(phy_blk_id, block_device)
}

// 设置起始扇区
pub fn set_start_sec(start_sec: usize) {
    BLOCK_CACHE_MANAGER.write().set_start_sec(start_sec);
}

// 写回磁盘，会调用Drop
pub fn write_to_dev() {
    BLOCK_CACHE_MANAGER.write().drop_all();
}

