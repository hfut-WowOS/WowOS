use crate::{ext, CommandExt};
use std::{ffi::OsStr, path::PathBuf, process::Command};

ext!(def; Git);

impl Git {
    fn new(sub: impl AsRef<OsStr>) -> Self {
        let mut git = Self(Command::new("git"));
        git.arg(sub);
        git
    }

    pub fn lfs() -> Self {
        Self::new("lfs")
    }

    pub fn config(global: bool) -> Self {
        let mut git = Self::new("config");
        git.option(global.then_some("--global"));
        git
    }

    pub fn clone(repo: impl AsRef<str>) -> GitCloneContext {
        GitCloneContext {
            repo: repo.as_ref().into(),
            dir: None,
            branch: None,
            single_branch: false,
            depth: usize::MAX,
        }
    }

    pub fn pull() -> Self {
        Self::new("pull")
    }

    pub fn submodule_update(init: bool) -> Self {
        let mut git = Self::new("submodule");
        git.arg("update").option(init.then_some("--init"));
        git
    }
}

pub struct GitCloneContext {
    repo: String,
    dir: Option<PathBuf>,
    branch: Option<String>,
    single_branch: bool,
    depth: usize,
}

impl GitCloneContext {
    #[inline]
    pub fn dir(mut self, path: PathBuf) -> Self {
        self.dir = Some(path);
        self
    }

    #[inline]
    pub fn branch(mut self, branch: impl AsRef<str>) -> Self {
        self.branch = Some(branch.as_ref().into());
        self
    }

    #[inline]
    pub fn single_branch(mut self) -> Self {
        self.single_branch = true;
        self
    }

    #[inline]
    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn done(self) -> Git {
        let mut git = Git::new("clone");
        git.arg(self.repo);
        if let Some(dir) = self.dir {
            git.arg(dir);
        }
        if let Some(branch) = self.branch {
            git.args(["--branch", &branch]);
        }
        if self.single_branch {
            git.arg("--single-branch");
        }
        if self.depth != usize::MAX {
            git.arg(format!("--depth={}", self.depth));
        }
        git
    }
}
