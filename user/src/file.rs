use super::*;

bitflags! {
    pub struct OpenFlags: u32 {
        const RDONLY = 0;           // 以只读模式 RDONLY 打开
        const WRONLY = 1 << 0;      // 以只写模式 WRONLY 打开
        const RDWR = 1 << 1;        // 既可读又可写
        const CREATE = 1 << 9;      // 允许创建文件 CREATE ，在找不到该文件的时候应创建文件；如果该文件已经存在则应该将该文件的大小归零；
        const TRUNC = 1 << 10;      // 在打开文件的时候应该清空文件的内容并将该文件的大小归零
    }
}

pub fn dup(fd: usize) -> isize {
    sys_dup(fd)
}
pub fn open(path: &str, flags: OpenFlags) -> isize {
    sys_open(path, flags.bits)
}
pub fn close(fd: usize) -> isize {
    sys_close(fd)
}
pub fn pipe(pipe_fd: &mut [usize]) -> isize {
    sys_pipe(pipe_fd)
}
pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf)
}
pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf)
}
