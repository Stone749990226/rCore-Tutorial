use super::BlockDevice;
use crate::mm::{
    frame_alloc, frame_dealloc, kernel_token, FrameTracker, PageTable, PhysAddr, PhysPageNum,
    StepByOne, VirtAddr,
};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;
use virtio_drivers::{Hal, VirtIOBlk, VirtIOHeader};

#[allow(unused)]
const VIRTIO0: usize = 0x10001000;

// 将 virtio-drivers crate 提供的 VirtIO 块设备抽象 VirtIOBlk 包装为我们自己的 VirtIOBlock ，实质上只是加上了一层互斥锁，生成一个新的类型来实现 easy-fs 需要的 BlockDevice Trait 
pub struct VirtIOBlock(UPSafeCell<VirtIOBlk<'static, VirtioHal>>);

// dma_alloc 中通过 frame_alloc 得到的那些物理页帧 FrameTracker 都会被保存在全局的向量 QUEUE_FRAMES 以延长它们的生命周期，避免提前被回收
lazy_static! {
    static ref QUEUE_FRAMES: UPSafeCell<Vec<FrameTracker>> = unsafe { UPSafeCell::new(Vec::new()) };
}

// 很容易为 VirtIOBlock 实现 BlockDevice Trait ，因为它内部来自 virtio-drivers crate 的 VirtIOBlk 类型已经实现了 read/write_block 方法，我们进行转发即可
impl BlockDevice for VirtIOBlock {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        self.0
            .exclusive_access()
            .read_block(block_id, buf)
            .expect("Error when reading VirtIOBlk");
    }
    fn write_block(&self, block_id: usize, buf: &[u8]) {
        self.0
            .exclusive_access()
            .write_block(block_id, buf)
            .expect("Error when writing VirtIOBlk");
    }
}

impl VirtIOBlock {
    #[allow(unused)]
    pub fn new() -> Self {
        unsafe {
            Self(UPSafeCell::new(
                // 在 VirtIOBlk::new 的时候需要传入一个 &mut VirtIOHeader 的参数， VirtIOHeader 实际上就代表以 MMIO 方式访问 VirtIO 设备所需的一组设备寄存器
                VirtIOBlk::<VirtioHal>::new(&mut *(VIRTIO0 as *mut VirtIOHeader)).unwrap(),
            ))
        }
    }
}

pub struct VirtioHal;

impl Hal for VirtioHal {
    // dma_alloc/dealloc 需要分配/回收数个 连续 的物理页帧，而我们的 frame_alloc 是逐个分配，严格来说并不保证分配的连续性。
    // 幸运的是，这个过程只会发生在内核初始化阶段，因此能够保证连续性
    fn dma_alloc(pages: usize) -> usize {
        let mut ppn_base = PhysPageNum(0);
        for i in 0..pages {
            let frame = frame_alloc().unwrap();
            if i == 0 {
                ppn_base = frame.ppn;
            }
            assert_eq!(frame.ppn.0, ppn_base.0 + i);
            QUEUE_FRAMES.exclusive_access().push(frame);
        }
        let pa: PhysAddr = ppn_base.into();
        pa.0
    }

    fn dma_dealloc(pa: usize, pages: usize) -> i32 {
        let pa = PhysAddr::from(pa);
        let mut ppn_base: PhysPageNum = pa.into();
        for _ in 0..pages {
            frame_dealloc(ppn_base);
            ppn_base.step();
        }
        0
    }

    fn phys_to_virt(addr: usize) -> usize {
        addr
    }

    fn virt_to_phys(vaddr: usize) -> usize {
        PageTable::from_token(kernel_token())
            .translate_va(VirtAddr::from(vaddr))
            .unwrap()
            .0
    }
}
