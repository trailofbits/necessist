use anyhow::Result;
use std::{fs::File, io::Error, path::Path};

use std::os::unix::io::AsRawFd;

pub fn lock_path(path: &Path) -> Result<File> {
    let file = File::open(path)?;
    lock_exclusive(&file)?;
    Ok(file)
}

pub fn try_lock_path(path: &Path) -> Result<File> {
    let file = File::open(path)?;
    try_lock_exclusive(&file)?;
    Ok(file)
}

// smoelius: `lock_exclusive`, `try_lock_exclusive`, and `flock` were copied from:
// https://github.com/rust-lang/cargo/blob/b0c9586f4cbf426914df47c65de38ea323772c74/src/cargo/util/flock.rs

fn lock_exclusive(file: &File) -> Result<()> {
    flock(file, libc::LOCK_EX)
}

fn try_lock_exclusive(file: &File) -> Result<()> {
    flock(file, libc::LOCK_EX | libc::LOCK_NB)
}

fn flock(file: &File, flag: libc::c_int) -> Result<()> {
    let ret = unsafe { libc::flock(file.as_raw_fd(), flag) };
    if ret < 0 {
        Err(Error::last_os_error().into())
    } else {
        Ok(())
    }
}
