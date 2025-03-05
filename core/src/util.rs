//! This module is semver exempt and its contents could change at any time.

use anyhow::{Context, Result};
use std::{
    env::current_dir,
    path::{Path, PathBuf},
    process::Command,
};
use subprocess::Exec;

pub struct RemoveFile(pub PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0)
            .map_err(|err| eprintln!("{err}"))
            .unwrap_or_default();
    }
}

/// Constructs a [`subprocess::Exec`] from a [`std::process::Command`].
pub fn exec_from_command(command: &Command) -> Exec {
    let mut exec = Exec::cmd(command.get_program()).args(&command.get_args().collect::<Vec<_>>());
    for (key, val) in command.get_envs() {
        if let Some(val) = val {
            exec = exec.env(key, val);
        } else {
            exec = exec.env_remove(key);
        }
    }
    if let Some(path) = command.get_current_dir() {
        exec = exec.cwd(path);
    }
    exec
}

/// Strips the current directory from the given path.
///
/// If the given path is not a child of the current directory, the path is
/// returned unchanged.
///
/// # Examples
///
/// ```
/// use necessist_core::util::strip_current_dir;
/// use std::{env::current_dir, path::Path};
///
/// let path = current_dir().unwrap().join("foo.txt");
/// let stripped = strip_current_dir(&path);
///
/// assert_eq!(stripped, Path::new("foo.txt"));
/// ```
#[must_use]
pub fn strip_current_dir(path: &Path) -> &Path {
    current_dir()
        .ok()
        .and_then(|dir| strip_prefix(path, &dir).ok())
        .unwrap_or(path)
}

/// Strip the prefix `base` from `path`.
///
/// # Errors
///
/// If `base` is not a prefix of `path`, an error is returned.
///
/// # Examples
///
/// ```
/// use necessist_core::util::strip_prefix;
/// use std::path::Path;
///
/// let path = Path::new("/a/b/c");
/// let base = Path::new("/a");
/// let stripped = strip_prefix(path, base).unwrap();
/// assert_eq!(stripped, Path::new("b/c"));
/// ```
pub fn strip_prefix<'a>(path: &'a Path, base: &Path) -> Result<&'a Path> {
    #[allow(clippy::disallowed_methods)]
    path.strip_prefix(base).with_context(|| {
        format!(
            "\
`base` is not a prefix of `path`
base: `{}`
path: `{}`",
            base.display(),
            path.display()
        )
    })
}
