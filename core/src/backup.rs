// smoelius: This file is a slight modification of:
// https://github.com/smoelius/rustfmt_if_chain/blob/557c32c54b0e0f48da2d029a3a8f70db4c8dbf9b/src/backup.rs

#![allow(dead_code)]
// smoelius: Allow `unwrap_used` until the following issue is resolved:
// https://github.com/rust-lang/rust-clippy/issues/10264
#![allow(clippy::unwrap_used)]
#![cfg_attr(dylint_lib = "overscoped_allow", allow(overscoped_allow))]

use std::{
    io::Result,
    path::{Path, PathBuf},
    time::SystemTime,
};
use tempfile::NamedTempFile;

pub struct Backup {
    path: PathBuf,
    tempfile: Option<NamedTempFile>,
}

impl Backup {
    pub fn new<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let tempfile = sibling_tempfile(path.as_ref())?;
        std::fs::copy(&path, &tempfile)?;
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            tempfile: Some(tempfile),
        })
    }

    pub fn disable(&mut self) -> Result<()> {
        self.tempfile.take().map_or(Ok(()), NamedTempFile::close)
    }
}

impl Drop for Backup {
    fn drop(&mut self) {
        if let Some(tempfile) = self.tempfile.take() {
            // smoelius: Ensure the file's mtime is updated, e.g., for build systems that rely on
            // this information. A useful relevant article: https://apenwarr.ca/log/20181113
            let before = mtime(&self.path).ok();
            loop {
                #[cfg(not(target_os = "macos"))]
                let result = std::fs::copy(&tempfile, &self.path);
                #[cfg(target_os = "macos")]
                let result = manual_copy(tempfile.path(), &self.path);
                if result.is_err() {
                    break;
                }
                let after = mtime(&self.path).ok();
                if before
                    .zip(after)
                    .map_or(true, |(before, after)| before < after)
                {
                    break;
                }
            }
        }
    }
}

fn mtime(path: &Path) -> Result<SystemTime> {
    path.metadata().and_then(|metadata| metadata.modified())
}

#[cfg(target_os = "macos")]
fn manual_copy(from: &Path, to: &Path) -> Result<()> {
    let contents = std::fs::read(from)?;
    std::fs::write(to, contents)
}

fn sibling_tempfile(path: &Path) -> Result<NamedTempFile> {
    let canonical_path = path.canonicalize()?;
    #[allow(clippy::expect_used)]
    let parent = canonical_path
        .parent()
        .expect("should not fail for a canonical path");
    NamedTempFile::new_in(parent)
}

#[cfg_attr(
    dylint_lib = "non_thread_safe_call_in_test",
    allow(non_thread_safe_call_in_test)
)]
#[test]
fn mtime_is_updated() {
    let tempfile = NamedTempFile::new().unwrap();

    let backup = Backup::new(&tempfile);

    let before = mtime(tempfile.path()).unwrap();

    drop(backup);

    let after = mtime(tempfile.path()).unwrap();

    assert!(before < after, "{before:?} not less than {after:?}");
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::{read_to_string, write};

    #[cfg_attr(
        dylint_lib = "non_thread_safe_call_in_test",
        allow(non_thread_safe_call_in_test)
    )]
    #[test]
    fn sanity() {
        let tempfile = NamedTempFile::new().unwrap();

        let backup = Backup::new(&tempfile).unwrap();

        write(&tempfile, "x").unwrap();

        assert_eq!(read_to_string(&tempfile).unwrap(), "x");

        drop(backup);

        assert!(read_to_string(&tempfile).unwrap().is_empty());
    }
}
