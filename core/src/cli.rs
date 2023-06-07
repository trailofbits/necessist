use crate::{framework, Necessist, Warning};
use clap::{crate_version, ArgAction, Parser, ValueEnum};
use std::path::PathBuf;

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Parser)]
#[clap(version = crate_version!())]
#[remain::sorted]
pub struct Opts<Identifier: Clone + Send + Sync + ValueEnum + 'static> {
    #[clap(
        long,
        action = ArgAction::Append,
        hide_possible_values = true,
        value_name = "WARNING",
        help = "Silence <WARNING>; `--allow all` silences all warnings"
    )]
    allow: Vec<Warning>,
    #[clap(
        long,
        help = "Create a default necessist.toml file in the project's root directory"
    )]
    default_config: bool,
    #[clap(
        long,
        action = ArgAction::Append,
        hide_possible_values = true,
        value_name = "WARNING",
        help = "Treat <WARNING> as an error; `--deny all` treats all warnings as errors"
    )]
    deny: Vec<Warning>,
    #[clap(long, help = "Dump sqlite database contents to the console")]
    dump: bool,
    #[clap(long, help = "Dump removal candidates and exit (for debugging)")]
    dump_candidates: bool,
    #[clap(long, help = "Assume testing framework is <FRAMEWORK>")]
    framework: Option<framework::Auto<Identifier>>,
    #[clap(long, help = "Do not perform dry runs")]
    no_dry_run: bool,
    #[clap(long, help = "Do not output to an sqlite database")]
    no_sqlite: bool,
    #[clap(long, help = "Do not output to the console")]
    quiet: bool,
    #[clap(long, help = "Discard sqlite database contents")]
    reset: bool,
    #[clap(long, help = "Resume from the sqlite database")]
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
    #[clap(
        last = true,
        name = "ARGS",
        help = "Additional arguments to pass to each test command"
    )]
    zzargs: Vec<String>,
}

impl<Identifier: Clone + Send + Sync + ValueEnum> From<Opts<Identifier>>
    for (Necessist, framework::Auto<Identifier>)
{
    fn from(opts: Opts<Identifier>) -> Self {
        let Opts {
            allow,
            default_config,
            deny,
            dump,
            dump_candidates,
            framework,
            no_dry_run,
            no_sqlite,
            quiet,
            reset,
            resume,
            root,
            timeout,
            verbose,
            ztest_files,
            zzargs,
        } = opts;
        let framework = framework.unwrap_or_default();
        let root = root.map(PathBuf::from);
        let test_files = ztest_files.iter().map(PathBuf::from).collect::<Vec<_>>();
        let args = zzargs;
        (
            Necessist {
                allow,
                default_config,
                deny,
                dump,
                dump_candidates,
                no_dry_run,
                no_sqlite,
                quiet,
                reset,
                resume,
                root,
                timeout,
                verbose,
                test_files,
                args,
            },
            framework,
        )
    }
}
