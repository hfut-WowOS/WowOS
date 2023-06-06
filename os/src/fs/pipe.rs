use alloc::sync::{Arc, Weak};
use spin::Mutex;
use crate::fs::File;
use crate::mm::UserBuffer;
use crate::task::suspend_current_and_run_next;

pub struct Pipe {
    readable: bool,
    writable: bool,
    buffer: Arc<Mutex<PipeRingBuffer>>,
}

impl Pipe {
    //管道的读端口
    pub fn read_end_with_buffer(buffer: Arc<Mutex<PipeRingBuffer>>) -> Self {
        Self {
            readable: true,
            writable: false,
            buffer,
        }
    }
    
    //写端口
    pub fn write_end_with_buffer(buffer: Arc<Mutex<PipeRingBuffer>>) -> Self {
        Self {
            readable: false,
            writable: true,
            buffer,
        }
    }
}

#[derive(Clone, PartialEq, Copy)]
enum RingBufferStatus {
    Full,
    Empty,
    Normal,
}

const RING_BUFFER_SIZE: usize = 32;

pub struct PipeRingBuffer {
    array: [u8; RING_BUFFER_SIZE],
    head: usize,
    tail: usize,
    status: RingBufferStatus,
    write_end: Option<Weak<Pipe>>,
}

impl PipeRingBuffer {
    pub fn new() -> Self {
        Self {
            array: [0; RING_BUFFER_SIZE],
            head: 0,
            tail: 0,
            status: RingBufferStatus::Empty,
            write_end: None,
        }
    }
    
    pub fn set_write_end(&mut self, write_end: &Arc<Pipe>) {
        self.write_end = Some(Arc::downgrade(write_end))
    }
    
    pub fn write_byte(&mut self, byte: u8) {
        self.status = RingBufferStatus::Normal;
        self.array[self.tail] = byte;
        self.tail = (self.tail + 1) % RING_BUFFER_SIZE;
        if self.head == self.tail {
            self.status = RingBufferStatus::Full
        }
    }
    
    pub fn read_byte(&mut self) -> u8 {
        self.status = RingBufferStatus::Normal;
        let byte = self.array[self.head];
        self.head = (self.head + 1) % RING_BUFFER_SIZE;
        if self.head == self.tail {
            self.status = RingBufferStatus::Empty
        }
        byte
    }
    
    pub fn available_read(&self) -> usize {
        if self.status == RingBufferStatus::Empty {
            0
        } else if self.tail > self.head {
            self.tail - self.head
        } else {
            RING_BUFFER_SIZE + self.tail - self.head
        }
    }
    
    pub fn available_write(&self) -> usize {
        if self.status == RingBufferStatus::Full {
            0
        } else {
            RING_BUFFER_SIZE - self.available_read()
        }
    }
    
    pub fn all_write_ends_closed(&self) -> bool {
        self.write_end.as_ref().unwrap().upgrade().is_none()
    }
}

impl File for Pipe {
    fn readable(&self) -> bool {
        self.readable
    }
    
    fn writable(&self) -> bool {
        self.writable
    }
    
    fn read(&self, buf: UserBuffer) -> usize {
        assert!(self.readable);
        let mut buf_iter = buf.into_iter();
        let mut total_read_size = 0;
        loop {
            let mut ring_buf = self.buffer.lock();
            //读取次数
            let read_turns = ring_buf.available_read();
            
            //当缓冲区读不出数据时让出cpu
            if read_turns == 0 {
                if ring_buf.all_write_ends_closed() {
                    return total_read_size;
                }
                drop(ring_buf);
                suspend_current_and_run_next();
                continue;
            }
            
            for _ in 0..read_turns {
                if let Some(byte_ref) = buf_iter.next() {
                    unsafe {
                        *byte_ref = ring_buf.read_byte();
                    }
                    total_read_size += 1;
                } else {
                    return total_read_size;
                }
            }
        }
    }
    
    fn write(&self, buf: UserBuffer) -> usize {
        assert!(self.writable());
        let mut buf_iter = buf.into_iter();
        let mut total_write_size = 0;
        
        loop {
            let mut ring_buf = self.buffer.lock();
            
            let write_turns = ring_buf.available_write();
            if write_turns == 0 {
                drop(ring_buf);
                suspend_current_and_run_next();
                continue;
            }
            
            for _ in 0..write_turns {
                if let Some(byte_ref) = buf_iter.next() {
                    ring_buf.write_byte(unsafe { *byte_ref });
                    total_write_size += 1;
                } else {
                    return total_write_size;
                }
            }
        }
    }
}

/// Return (read_end, write_end)
pub fn make_pipe() -> (Arc<Pipe>, Arc<Pipe>) {
    let buffer = Arc::new(Mutex::new(PipeRingBuffer::new()));
    let read_end = Arc::new(Pipe::read_end_with_buffer(buffer.clone()));
    let write_end = Arc::new(Pipe::write_end_with_buffer(buffer.clone()));
    buffer.lock().set_write_end(&write_end);
    (read_end, write_end)
}
