use super::{BlockDevice, BLOCK_SZ};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
use spin::Mutex;
/// Cached block inside memory
pub struct BlockCache {
    // cache 是一个 512 字节的数组，表示位于内存中的缓冲区
    /// cached block data
    cache: [u8; BLOCK_SZ],
    // block_id 记录了这个块缓存来自于磁盘中的块的编号
    /// underlying block id
    block_id: usize,
    // block_device 是一个底层块设备的引用，可通过它进行块读写
    /// underlying block device
    block_device: Arc<dyn BlockDevice>,
    // modified 记录这个块从磁盘载入内存缓存之后，它有没有被修改过
    /// whether the block is dirty
    modified: bool,
}

// 一旦磁盘块已经存在于内存缓存中，CPU 就可以直接访问磁盘块数据了
impl BlockCache {
    // 当我们创建一个 BlockCache 的时候，这将触发一次 read_block 将一个块上的数据从磁盘读到缓冲区 cache
    /// Load a new BlockCache from disk.
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
    /// Get the address of an offset inside the cached block data
    fn addr_of_offset(&self, offset: usize) -> usize {
        &self.cache[offset] as *const _ as usize
    }

    // 获取缓冲区中的位于偏移量 offset 的一个类型为 T 的磁盘上数据结构的不可变引用
    pub fn get_ref<T>(&self, offset: usize) -> &T
    // Trait Bound 限制类型 T 必须是一个编译时已知大小的类型
    where
        T: Sized,
    {
        // 通过 core::mem::size_of::<T>() 在编译时获取类型 T 的大小
        let type_size = core::mem::size_of::<T>();
        // 确认该数据结构被整个包含在磁盘块及其缓冲区之内
        assert!(offset + type_size <= BLOCK_SZ);
        let addr = self.addr_of_offset(offset);
        unsafe { &*(addr as *const T) }
    }
    // get_mut 会获取磁盘上数据结构的可变引用，由此可以对数据结构进行修改
    pub fn get_mut<T>(&mut self, offset: usize) -> &mut T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        // 将 BlockCache 的 modified 标记为 true 表示该缓冲区已经被修改，之后需要将数据写回磁盘块才能真正将修改同步到磁盘
        self.modified = true;
        let addr = self.addr_of_offset(offset);
        unsafe { &mut *(addr as *mut T) }
    }

    // 将 get_ref/get_mut 进一步封装为更为易用的形式：
    // 在 BlockCache 缓冲区偏移量为 offset 的位置获取一个类型为 T 的磁盘上数据结构的不可变/可变引用（分别对应 read/modify ），并让它执行传入的闭包 f 中所定义的操作
    // 相当于 read/modify 构成了传入闭包 f 的一层执行环境，让它能够绑定到一个缓冲区上执行
    // 这里我们传入闭包的类型为 FnOnce ，这是因为闭包里面的变量被捕获的方式涵盖了不可变引用/可变引用/和 move 三种可能性，故而我们需要选取范围最广的 FnOnce 。
    // 参数中的 impl 关键字体现了一种类似泛型的静态分发功能
    pub fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    pub fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    // 在 Linux 中，sync 并不是只有在 drop 的时候才会被调用。通常有一个后台进程负责定期将内存中缓冲区的内容写回磁盘。另外有一个 sys_fsync 系统调用可以让应用主动通知内核将一个文件的修改同步回磁盘。
    // 由于我们的实现比较简单， sync 仅会在 BlockCache 被 drop 时才会被调用
    pub fn sync(&mut self) {
        // modified 标记将会决定数据是否需要写回磁盘
        if self.modified {
            self.modified = false;
            self.block_device.write_block(self.block_id, &self.cache);
        }
    }
}

// BlockCache 的设计也体现了 RAII 思想， 它管理着一个缓冲区的生命周期。当 BlockCache 的生命周期结束之后缓冲区也会被从内存中回收
impl Drop for BlockCache {
    fn drop(&mut self) {
        self.sync()
    }
}
// 为了避免在块缓存上浪费过多内存，我们希望内存中同时只能驻留有限个磁盘块的缓冲区
/// Use a block cache of 16 blocks
const BLOCK_CACHE_SIZE: usize = 16;

// 块缓存全局管理器的功能是：当我们要对一个磁盘块进行读写时，首先看它是否已经被载入到内存缓存中了，如果已经被载入的话则直接返回，否则需要先读取磁盘块的数据到内存缓存中
// 如果内存中驻留的磁盘块缓冲区的数量已满，则需要遵循某种缓存替换算法将某个块的缓存从内存中移除，再将刚刚读到的块数据加入到内存缓存中。
// 我们这里使用一种类 FIFO 的简单缓存替换算法，因此在管理器中只需维护一个队列：
pub struct BlockCacheManager {
    // 队列 queue 中管理的是块编号和块缓存的二元组。块编号的类型为 usize ，而块缓存的类型则是一个 Arc<Mutex<BlockCache>>
    // Arc和Mutex组合可以同时提供共享引用和互斥访问
    // 共享引用意义在于块缓存既需要在管理器 BlockCacheManager 保留一个引用，还需要以引用的形式返回给块缓存的请求者让它可以对块缓存进行访问
    queue: VecDeque<(usize, Arc<Mutex<BlockCache>>)>,
}

impl BlockCacheManager {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    // 从块缓存管理器中获取一个编号为 block_id 的块的块缓存，如果找不到，会从磁盘读取到内存中，还有可能会发生缓存替换
    pub fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<Mutex<BlockCache>> {
        // 遍历整个队列试图找到一个编号相同的块缓存，如果找到了，会将块缓存管理器中保存的块缓存的引用复制一份并返回
        if let Some(pair) = self.queue.iter().find(|pair: &&(usize, Arc<Mutex<BlockCache>>)| pair.0 == block_id) {
            Arc::clone(&pair.1)
        } else {
            // 找不到时，必须将块从磁盘读入内存中的缓冲区。在实际读取之前，需要判断管理器保存的块缓存数量是否已经达到了上限
            // substitute
            if self.queue.len() == BLOCK_CACHE_SIZE {
                // 如果达到了上限需要执行缓存替换算法，丢掉某个块缓存并空出一个空位。从队头遍历到队尾找到第一个强引用计数恰好为 1 的块缓存并将其替换出去
                // from front to tail
                if let Some((idx, _)) = self
                    .queue
                    .iter()
                    .enumerate()
                    .find(|(_, pair)| Arc::strong_count(&pair.1) == 1)
                {
                    self.queue.drain(idx..=idx);
                } else {
                    // 队列已满且其中所有的块缓存都正在使用的情形，内核将 panic （基于简单内核设计的思路）
                    panic!("Run out of BlockCache!");
                }
            }
            // 创建一个新的块缓存（会触发 read_block 进行块读取）并加入到队尾，最后返回给请求者
            // load block into mem and push back
            let block_cache = Arc::new(Mutex::new(BlockCache::new(
                block_id,
                Arc::clone(&block_device),
            )));
            self.queue.push_back((block_id, Arc::clone(&block_cache)));
            block_cache
        }
    }
}

// 创建 BlockCacheManager 的全局实例：
lazy_static! {
    /// The global block cache manager
    pub static ref BLOCK_CACHE_MANAGER: Mutex<BlockCacheManager> =
        Mutex::new(BlockCacheManager::new());
}
// 对于其他模块而言，就可以直接通过 get_block_cache 方法来请求块缓存了。
// 它返回的是一个 Arc<Mutex<BlockCache>> ，调用者需要通过 .lock() 获取里层互斥锁 Mutex 才能对最里面的 BlockCache 进行操作
/// Get the block cache corresponding to the given block id and block device
pub fn get_block_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
) -> Arc<Mutex<BlockCache>> {
    BLOCK_CACHE_MANAGER
        .lock()
        .get_block_cache(block_id, block_device)
}
/// Sync all block cache to block device
pub fn block_cache_sync_all() {
    let manager = BLOCK_CACHE_MANAGER.lock();
    for (_, cache) in manager.queue.iter() {
        cache.lock().sync();
    }
}
