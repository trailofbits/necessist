use anyhow::Result;
use clap::{crate_version, ArgEnum, Parser};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::{
    env::{args, var},
    fs::{File, OpenOptions},
    io::Error,
    path::{Path, PathBuf},
};

#[derive(ArgEnum, Clone, Copy, Debug)]
#[remain::sorted]
enum Framework {
    Auto,
    HardhatTs,
    Rust,
}

impl Default for Framework {
    fn default() -> Self {
        Framework::Auto
    }
}

impl From<Framework> for necessist::Framework {
    fn from(framework: Framework) -> Self {
        match framework {
            Framework::Auto => Self::Auto,
            Framework::Rust => Self::Rust,
            Framework::HardhatTs => Self::HardhatTs,
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Parser)]
#[clap(version = crate_version!())]
#[remain::sorted]
struct Opts {
    #[clap(long, help = "Dump sqlite database contents to the console")]
    dump: bool,
    #[clap(long, arg_enum, help = "Assume testing framework is <FRAMEWORK>")]
    framework: Option<Framework>,
    #[clap(long, help = "Continue when a dry run fails or a test cannot be run")]
    keep_going: bool,
    #[clap(long, help = "Do not perform dry runs")]
    no_dry_run: bool,
    #[clap(long, help = "Do not resume from or output to sqlite database")]
    no_sqlite: bool,
    #[clap(long, help = "Do not output to the console")]
    quiet: bool,
    #[clap(long, help = "Discard sqlite database contents")]
    reset: bool,
    #[clap(long, help = "Resume from sqlite database")]
    resume: bool,
    #[clap(long, help = "Root directory of the project under test")]
    root: Option<String>,
    #[clap(
        long,
        help = "Maximum number of seconds to run any test; 60 is the default, 0 means no timeout"
    )]
    timeout: Option<u64>,
    #[clap(long, help = "Show test outcomes besides `passed`")]
    verbose: bool,
    #[clap(value_name = "TEST_FILES", help = "Test files to mutilate (optional)")]
    ztest_files: Vec<String>,
}

impl From<Opts> for necessist::Necessist {
    fn from(opts: Opts) -> Self {
        let Opts {
            dump,
            framework,
            keep_going,
            no_dry_run,
            no_sqlite,
            quiet,
            reset,
            resume,
            root,
            timeout,
            verbose,
            ztest_files,
        } = opts;
        let framework = framework.unwrap_or_default().into();
        let root = root.map(PathBuf::from);
        let test_files = ztest_files.iter().map(PathBuf::from).collect::<Vec<_>>();
        Self {
            dump,
            framework,
            keep_going,
            no_dry_run,
            no_sqlite,
            quiet,
            reset,
            resume,
            root,
            timeout,
            verbose,
            test_files,
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();

    let opts: necessist::Necessist = Opts::parse_from(args()).into();

    // smoelius: Prevent `trycmd` tests from running concurrently.
    #[cfg(unix)]
    let _file = if enabled("TRYCMD") {
        if let Some(root) = &opts.root {
            let file = lock_path(root)?;
            Some(file)
        } else {
            None
        }
    } else {
        None
    };

    necessist::necessist(&opts)
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

// smoelius: `lock_exclusive` and `flock` were copied from:
// https://github.com/rust-lang/cargo/blob/b0c9586f4cbf426914df47c65de38ea323772c74/src/cargo/util/flock.rs

fn lock_exclusive(file: &File) -> Result<()> {
    flock(file, libc::LOCK_EX)
}

fn flock(file: &File, flag: libc::c_int) -> Result<()> {
    let ret = unsafe { libc::flock(file.as_raw_fd(), flag) };
    if ret < 0 {
        Err(Error::last_os_error().into())
    } else {
        Ok(())
    }
}
