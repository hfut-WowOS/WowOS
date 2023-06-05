//! 操作目录。

use std::{
    fs,
    io::{ErrorKind, Result},
    path::Path,
};

/// 删除指定路径。
///
/// 如果返回 `Ok(())`，`path` 将不存在。
pub fn rm(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if let Err(e) = fs::remove_file(path) {
        if matches!(e.kind(), ErrorKind::NotFound) {
            Ok(())
        } else {
            Err(e)
        }
    } else {
        Ok(())
    }
}

/// 创建 `path` 的父目录。
#[inline]
pub fn create_parent(path: impl AsRef<Path>) -> Result<()> {
    match path.as_ref().parent() {
        Some(parent) => fs::create_dir_all(parent),
        None => Ok(()),
    }
}

/// 清空 `path` 目录。
///
/// 如果返回 `Ok(())`，`path` 将是一个存在的空目录。
#[inline]
pub fn clear(path: impl AsRef<Path>) -> Result<()> {
    rm(&path)?;
    std::fs::create_dir_all(&path)
}
