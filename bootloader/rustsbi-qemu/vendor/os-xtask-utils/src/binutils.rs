use crate::{ext, Cargo, CommandExt};
use std::{ffi::OsStr, process::Command};

ext!(def; BinUtil);

impl BinUtil {
    fn new(which: impl AsRef<OsStr>) -> Self {
        let which = which.as_ref();
        let installed = Cargo::install().arg("--list").output().stdout;
        let check = String::from_utf8_lossy(&installed)
            .lines()
            .filter_map(|line| {
                if cfg!(target_os = "windows") {
                    line.trim().strip_suffix(".exe")
                } else {
                    Some(line.trim())
                }
            })
            .any(|line| OsStr::new(line) == which);
        if !check {
            Cargo::install().arg("cargo-binutils").invoke();
        }
        Self(Command::new(which))
    }

    #[inline]
    pub fn objcopy() -> Self {
        Self::new("rust-objcopy")
    }

    #[inline]
    pub fn objdump() -> Self {
        Self::new("rust-objdump")
    }
}
