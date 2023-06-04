/// os/src/drivers/block/virtio_blk.rs
/// 定义了一个名为VirtIOBlock的结构体，实现了BlockDevice trait。
/// 它使用了VirtIO驱动程序来实现块设备的读取和写入。

use super::BlockDevice;
use crate::drivers::bus::virtio::VirtioHal;
use crate::sync::{Condvar, UPIntrFreeCell};
use crate::task::schedule;
use crate::DEV_NON_BLOCKING_ACCESS;
use alloc::collections::BTreeMap;
use virtio_drivers::{BlkResp, RespStatus, VirtIOBlk, VirtIOHeader};

#[allow(unused)]
const VIRTIO0: usize = 0x10008000;

pub struct VirtIOBlock {
    // 表示VirtIO块设备对象。它是一个带有中断和自由访问权限的封装结构体。
    virtio_blk: UPIntrFreeCell<VirtIOBlk<'static, VirtioHal>>,
    // 表示条件变量的集合, 用于在非阻塞访问模式下等待I/O操作完成。
    condvars: BTreeMap<u16, Condvar>,
}

impl BlockDevice for VirtIOBlock {
    /// 根据DEV_NON_BLOCKING_ACCESS的值，它可以选择阻塞或非阻塞模式进行访问。
    /// 如果处于非阻塞模式，它会发起一个异步读取请求，并通过条件变量等待请求完成后继续执行。
    /// 如果处于阻塞模式，它会直接调用VirtIO块设备对象的read_block方法进行读取。
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        let nb = *DEV_NON_BLOCKING_ACCESS.exclusive_access();
        if nb {
            let mut resp = BlkResp::default();
            let task_cx_ptr = self.virtio_blk.exclusive_session(|blk| {
                let token = unsafe { blk.read_block_nb(block_id, buf, &mut resp).unwrap() };
                self.condvars.get(&token).unwrap().wait_no_sched()
            });
            schedule(task_cx_ptr);
            assert_eq!(
                resp.status(),
                RespStatus::Ok,
                "Error when reading VirtIOBlk"
            );
        } else {
            self.virtio_blk
                .exclusive_access()
                .read_block(block_id, buf)
                .expect("Error when reading VirtIOBlk");
        }
    }

    /// 向块设备中写入数据块
    /// 根据DEV_NON_BLOCKING_ACCESS的值选择阻塞或非阻塞模式进行访问。
    fn write_block(&self, block_id: usize, buf: &[u8]) {
        let nb = *DEV_NON_BLOCKING_ACCESS.exclusive_access();
        if nb {
            let mut resp = BlkResp::default();
            let task_cx_ptr = self.virtio_blk.exclusive_session(|blk| {
                let token = unsafe { blk.write_block_nb(block_id, buf, &mut resp).unwrap() };
                self.condvars.get(&token).unwrap().wait_no_sched()
            });
            schedule(task_cx_ptr);
            assert_eq!(
                resp.status(),
                RespStatus::Ok,
                "Error when writing VirtIOBlk"
            );
        } else {
            self.virtio_blk
                .exclusive_access()
                .write_block(block_id, buf)
                .expect("Error when writing VirtIOBlk");
        }
    }
    /// 用于处理中断。
    /// 它会从VirtIO块设备对象的已使用队列中弹出已完成的请求，并通过条件变量发出信号，以唤醒等待的线程。
    fn handle_irq(&self) {
        self.virtio_blk.exclusive_session(|blk| {
            while let Ok(token) = blk.pop_used() {
                self.condvars.get(&token).unwrap().signal();
            }
        });
    }
}

impl VirtIOBlock {
    pub fn new() -> Self {
        let virtio_blk = unsafe {
            UPIntrFreeCell::new(
                VirtIOBlk::<VirtioHal>::new(&mut *(VIRTIO0 as *mut VirtIOHeader)).unwrap(),
            )
        };
        let mut condvars = BTreeMap::new();
        let channels = virtio_blk.exclusive_access().virt_queue_size();
        for i in 0..channels {
            let condvar = Condvar::new();
            condvars.insert(i, condvar);
        }
        Self {
            virtio_blk,
            condvars,
        }
    }
}
