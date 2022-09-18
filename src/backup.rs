// smoelius: This file is a slight modification of:
// https://github.com/smoelius/rustfmt_if_chain/blob/557c32c54b0e0f48da2d029a3a8f70db4c8dbf9b/src/backup.rs

#![allow(dead_code)]

use std::{
    fs::copy,
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
        copy(&path, &tempfile)?;
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
                if copy(&tempfile, &self.path).is_err() {
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

#[allow(clippy::expect_used)]
fn sibling_tempfile(path: &Path) -> Result<NamedTempFile> {
    let canonical_path = path.canonicalize()?;
    let parent = canonical_path
        .parent()
        .expect("should not fail for a canonical path");
    NamedTempFile::new_in(parent)
}

#[test]
fn mtime_is_updated() {
    let tempfile = NamedTempFile::new().unwrap();

    let backup = Backup::new(tempfile.path());

    let before = mtime(tempfile.path()).unwrap();

    drop(backup);

    let after = mtime(tempfile.path()).unwrap();

    assert!(before < after, "{:?} not less than {:?}", before, after);
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::{read_to_string, write};

    #[test]
    fn sanity() {
        let tempfile = NamedTempFile::new().unwrap();

        let backup = Backup::new(tempfile.path()).unwrap();

        write(tempfile.path(), "x").unwrap();

        assert_eq!(read_to_string(tempfile.path()).unwrap(), "x");

        drop(backup);

        assert!(read_to_string(tempfile.path()).unwrap().is_empty());
    }
}
