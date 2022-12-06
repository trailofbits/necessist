#![warn(clippy::expect_used)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]

use anyhow::Result;
use clap::Parser;
use log::debug;
use necessist_core::{cli, necessist, AutoUnion, Identifier, Necessist};
use std::{
    env::{args, var},
    fs::{File, OpenOptions},
    io::Error,
    path::Path,
};

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

mod framework;

fn main() -> Result<()> {
    env_logger::init();

    let (opts, framework): (Necessist, AutoUnion<Identifier, framework::Identifier>) =
        cli::Opts::parse_from(args()).into();

    // smoelius: Prevent `trycmd` tests from running concurrently.
    #[cfg(unix)]
    let _file = if enabled("TRYCMD") {
        if let Some(root) = &opts.root {
            debug!("Locking {:?}", root);
            let file = try_lock_path(root).or_else(|error| {
                debug!("Failed to lock {:?}: {}", root, error);
                lock_path(root)
            })?;
            Some(file)
        } else {
            None
        }
    } else {
        None
    };

    necessist(&opts, framework)
}

#[must_use]
pub fn enabled(key: &str) -> bool {
    var(key).map_or(false, |value| value != "0")
}

fn lock_path(path: &Path) -> Result<File> {
    let file = OpenOptions::new().read(true).open(path)?;
    lock_exclusive(&file)?;
    Ok(file)
}

fn try_lock_path(path: &Path) -> Result<File> {
    let file = OpenOptions::new().read(true).open(path)?;
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
