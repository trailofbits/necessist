//! This module is semver exempt and its contents could change at any time.

use anyhow::{Context, Result};
use std::{
    env::current_dir,
    path::{Path, PathBuf},
};

pub struct RemoveFile(pub PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0)
            .map_err(|err| eprintln!("{err}"))
            .unwrap_or_default();
    }
}

#[must_use]
pub fn strip_current_dir(path: &Path) -> &Path {
    current_dir()
        .ok()
        .and_then(|dir| strip_prefix(path, &dir).ok())
        .unwrap_or(path)
}

pub fn strip_prefix<'a>(path: &'a Path, base: &Path) -> Result<&'a Path> {
    #[allow(clippy::disallowed_methods)]
    path.strip_prefix(base).with_context(|| {
        format!(
            "\
`base` is not a prefix of `path`
base: {base:?}
path: {path:?}"
        )
    })
}
