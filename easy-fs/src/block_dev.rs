use core::any::Any;
/// Trait for block devices
/// which reads and writes data in the unit of blocks
// Any trait 是一个 trait 对象，它允许类型安全地对任何类型进行类型检查和类型转换
// Send 和 Sync trait 限定符表示实现了这个 trait 的类型可以安全地在多个线程之间传递（Send）和共享（Sync）
pub trait BlockDevice: Send + Sync + Any {
    ///Read data form block to buffer
    fn read_block(&self, block_id: usize, buf: &mut [u8]);
    ///Write data from buffer to block
    fn write_block(&self, block_id: usize, buf: &[u8]);
}
