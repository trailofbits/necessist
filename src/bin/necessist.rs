use anyhow::Result;
use clap::{crate_version, ArgEnum, Parser};
use necessist::{necessist, Necessist};
use std::{env::args, path::PathBuf};

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
    #[clap(long, help = "Dump the contents of the sqlite database to the console")]
    dump: bool,
    #[clap(long, arg_enum, help = "Assume testing framework is <FRAMEWORK>")]
    framework: Option<Framework>,
    #[clap(long, help = "Continue when a dry run fails or a test cannot be run")]
    keep_going: bool,
    #[clap(long, help = "Do not perform dry runs")]
    no_dry_run: bool,
    #[clap(long, help = "Do not output to the console")]
    quiet: bool,
    #[clap(long, help = "Resume from the sqlite database; implies --sqlite")]
    resume: bool,
    #[clap(long, help = "Root directory of the project under test")]
    root: Option<String>,
    #[clap(long, help = "Output to a sqlite database in addition to the console")]
    sqlite: bool,
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

impl From<Opts> for Necessist {
    fn from(opts: Opts) -> Self {
        let Opts {
            dump,
            framework,
            keep_going,
            no_dry_run,
            quiet,
            resume,
            root,
            sqlite,
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
            quiet,
            resume,
            root,
            sqlite,
            timeout,
            verbose,
            test_files,
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();

    necessist(&Opts::parse_from(args()).into())
}
