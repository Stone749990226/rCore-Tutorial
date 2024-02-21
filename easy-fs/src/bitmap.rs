use super::{get_block_cache, BlockDevice, BLOCK_SZ};
use alloc::sync::Arc;
// 磁盘块上位图区域的数据是要以磁盘数据结构 BitmapBlock 的格式进行操作
// BitmapBlock 是一个磁盘数据结构，它将位图区域中的一个磁盘块解释为长度为 64 的一个 u64 数组，每个 u64 打包了一组 64 bits，于是整个数组包含 64×64=4096 bits，且可以以组为单位进行操作
/// A bitmap block
type BitmapBlock = [u64; 64];
/// Number of bits in a block
const BLOCK_BITS: usize = BLOCK_SZ * 8;

// 位图 Bitmap 中仅保存了它所在区域的起始块编号以及区域的长度为多少个块
// Bitmap 自身是驻留在内存中的，但是它能够表示索引节点/数据块区域中的那些磁盘块的分配情况
/// A bitmap
pub struct Bitmap {
    start_block_id: usize,
    blocks: usize,
}

/// Decompose bits into (block_pos, bits64_pos, inner_pos)
fn decomposition(mut bit: usize) -> (usize, usize, usize) {
    let block_pos = bit / BLOCK_BITS;
    bit %= BLOCK_BITS;
    (block_pos, bit / 64, bit % 64)
}

impl Bitmap {
    /// A new bitmap from start block id and number of blocks
    pub fn new(start_block_id: usize, blocks: usize) -> Self {
        Self {
            start_block_id,
            blocks,
        }
    }
    /// Allocate a new block from a block device
    pub fn alloc(&self, block_device: &Arc<dyn BlockDevice>) -> Option<usize> {
        // 遍历区域中的每个块
        for block_id in 0..self.blocks {
            let pos = get_block_cache(
                block_id + self.start_block_id as usize,
                Arc::clone(block_device),
            )
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                // 再在每个块中以bit组（每组 64 bits）为单位进行遍历
                // 如果它并没有达到 u64::MAX （即 2^64-1），则通过 u64::trailing_ones 找到最低的一个 0 并置为 1
                // 如果能够找到的话，bit组的编号将保存在变量 bits64_pos 中，而分配的bit在组内的位置将保存在变量 inner_pos 中
                if let Some((bits64_pos, inner_pos)) = bitmap_block
                    .iter()
                    .enumerate()
                    .find(|(_, bits64)| **bits64 != u64::MAX)
                    .map(|(bits64_pos, bits64)| (bits64_pos, bits64.trailing_ones() as usize))
                {
                    // modify cache
                    bitmap_block[bits64_pos] |= 1u64 << inner_pos;
                    Some(block_id * BLOCK_BITS + bits64_pos * 64 + inner_pos as usize)
                } else {
                    None
                }
            });
            if pos.is_some() {
                // 一旦在某个块中找到一个空闲的bit并成功分配，就不再考虑后续的块，提前返回
                return pos;
            }
        }
        None
    }
    /// Deallocate a block
    pub fn dealloc(&self, block_device: &Arc<dyn BlockDevice>, bit: usize) {
        let (block_pos, bits64_pos, inner_pos) = decomposition(bit);
        get_block_cache(block_pos + self.start_block_id, Arc::clone(block_device))
            .lock()
            .modify(0, |bitmap_block: &mut BitmapBlock| {
                assert!(bitmap_block[bits64_pos] & (1u64 << inner_pos) > 0);
                bitmap_block[bits64_pos] -= 1u64 << inner_pos;
            });
    }
    /// Get the max number of allocatable blocks
    pub fn maximum(&self) -> usize {
        self.blocks * BLOCK_BITS
    }
}
