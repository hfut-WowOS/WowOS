#![deny(warnings, unsafe_code)]

mod binutils;
mod cargo;
pub mod dir;
mod git;
mod make;
mod qemu;
mod tar;

pub use binutils::BinUtil;
pub use cargo::Cargo;
pub use git::Git;
pub use make::Make;
pub use qemu::Qemu;
pub use tar::Tar;

use std::{
    ffi::{OsStr, OsString},
    path::Path,
    process::{Command, ExitStatus, Output},
};

pub trait CommandExt: AsRef<Command> + AsMut<Command> {
    #[inline]
    fn arg(&mut self, s: impl AsRef<OsStr>) -> &mut Self {
        self.as_mut().arg(s);
        self
    }

    #[inline]
    fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.as_mut().args(args);
        self
    }

    #[inline]
    fn optional<T>(&mut self, option: &Option<T>, op: impl FnOnce(&mut Self, &T)) -> &mut Self {
        if let Some(val) = option {
            op(self, val);
        }
        self
    }

    #[inline]
    fn conditional(&mut self, condition: bool, op: impl FnOnce(&mut Self)) -> &mut Self {
        if condition {
            op(self);
        }
        self
    }

    #[inline]
    fn option(&mut self, option: Option<impl AsRef<OsStr>>) -> &mut Self {
        if let Some(arg) = option {
            self.as_mut().arg(arg);
        }
        self
    }

    #[inline]
    fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.as_mut().current_dir(dir);
        self
    }

    #[inline]
    fn env(&mut self, key: impl AsRef<OsStr>, val: impl AsRef<OsStr>) -> &mut Self {
        self.as_mut().env(key, val);
        self
    }

    #[inline]
    fn status(&mut self) -> ExitStatus {
        self.as_mut().status().unwrap()
    }

    fn info(&self) -> OsString {
        let cmd = self.as_ref();
        let mut msg = OsString::new();
        if let Some(dir) = cmd.get_current_dir() {
            msg.push("cd ");
            msg.push(dir);
            msg.push(" && ");
        }
        msg.push(cmd.get_program());
        for a in cmd.get_args() {
            msg.push(" ");
            msg.push(a);
        }
        for (k, v) in cmd.get_envs() {
            msg.push(" ");
            msg.push(k);
            if let Some(v) = v {
                msg.push("=");
                msg.push(v);
            }
        }
        msg
    }

    #[inline]
    fn invoke(&mut self) {
        let status = self.status();
        if !status.success() {
            panic!(
                "Failed with code {}: {:?}",
                status.code().unwrap(),
                self.info()
            );
        }
    }

    #[inline]
    fn output(&mut self) -> Output {
        let output = self.as_mut().output().unwrap();
        if !output.status.success() {
            panic!(
                "Failed with code {}: {:?}",
                output.status.code().unwrap(),
                self.info()
            );
        }
        output
    }
}

ext!(def; Ext);

impl Ext {
    #[inline]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self(Command::new(program))
    }
}

mod m {
    #[macro_export]
    macro_rules! ext {
        (def; $name:ident) => {
            pub struct $name(std::process::Command);

            ext!($name);
        };

        ($ty:ty) => {
            impl AsRef<Command> for $ty {
                fn as_ref(&self) -> &Command {
                    &self.0
                }
            }

            impl AsMut<Command> for $ty {
                fn as_mut(&mut self) -> &mut Command {
                    &mut self.0
                }
            }

            impl $crate::CommandExt for $ty {}
        };
    }
}
