#[allow(unused)]
// rcore 指导书中efs的代码，用于学习：实现了一个位图数据结构 Bitmap，用于管理分配和释放位图中的位
use super::{get_block_cache, BlockDevice, BLOCK_SZ};
use alloc::sync::Arc;

// BitmapBlock 是一个磁盘数据结构，它将位图区域中的一个磁盘块解释为长度为 64 的一个 u64 数组， 每个 u64 打包了一组 64 bits
type BitmapBlock = [u64; 64];

const BLOCK_BITS: usize = BLOCK_SZ * 8;

/// BitmapBlock 类型是一个包含 64 个 u64 元素的数组，用于表示位图块的数据。BLOCK_BITS 是每个块的位数，
/// 由 BLOCK_SZ 的值乘以 8 得到。Bitmap 结构体表示位图，
/// 包含起始块标识符 start_block_id 和块数 blocks。
pub struct Bitmap {
    start_block_id: usize,
    blocks: usize, //块数
}

/// Return (block_pos, bits64_pos, inner_pos)
fn decomposition(mut bit: usize) -> (usize, usize, usize) {
    let block_pos = bit / BLOCK_BITS;
    bit = bit % BLOCK_BITS;
    (block_pos, bit / 64, bit % 64)
}

impl Bitmap {
    /// new 方法是一个构造函数，用于创建新的 Bitmap 实例。它以起始块标识符 start_block_id 和块数 blocks 作为参数，并返回一个 Bitmap 实例。
    pub fn new(start_block_id: usize, blocks: usize) -> Self {
        Self {
            start_block_id,
            blocks,
        }
    }

    /// alloc 方法用于分配一个可用位。它以 block_device 参数作为底层块设备，并遍历位图的每个块来查找未被使用的位。一旦找到未被使用的位，就在缓存中修改位图块，并返回该位的位置。如果没有可用的位，返回 None。
    pub fn alloc(&self, block_device: &Arc<dyn BlockDevice>) -> Option<usize> {
        for block_id in 0..self.blocks {
            let pos = get_block_cache(
                block_id + self.start_block_id as usize,
                Arc::clone(block_device),
            )
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                if let Some((bits64_pos, inner_pos)) = bitmap_block
                    .iter()
                    .enumerate()
                    .find(|(_, bits64)| **bits64 != u64::MAX) //找一个有空闲的
                    .map(|(bits64_pos, bits64)| {
                        (bits64_pos, bits64.trailing_ones() as usize) //找到最低的0位并置1
                    })
                {
                    // modify cache
                    bitmap_block[bits64_pos] |= 1u64 << inner_pos;
                    Some(block_id * BLOCK_BITS + bits64_pos * 64 + inner_pos as usize)
                } else {
                    None
                }
            });
            if pos.is_some() {
                return pos;
            }
        }
        None
    }

    /// dealloc 方法用于释放一个已分配的位。它以 block_device 参数作为底层块设备，并根据位的位置将对应的位图块从缓存中取出，并将相应位位置的位清零。
    pub fn dealloc(&self, block_device: &Arc<dyn BlockDevice>, bit: usize) {
        let (block_pos, bits64_pos, inner_pos) = decomposition(bit);
        get_block_cache(block_pos + self.start_block_id, Arc::clone(block_device))
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                assert!(bitmap_block[bits64_pos] & (1u64 << inner_pos) > 0);
                bitmap_block[bits64_pos] -= 1u64 << inner_pos;
            });
    }

    /// maximum 方法返回位图中可用位的最大数量，即位图的总位数。
    pub fn maximum(&self) -> usize {
        self.blocks * BLOCK_BITS
    }
}