use crate::{framework, Necessist, Warning};
use clap::{crate_version, ArgAction, Parser, ValueEnum};
use std::path::PathBuf;

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Parser)]
#[clap(version = crate_version!())]
#[remain::sorted]
pub struct Opts<AdditionalIdentifier: Clone + Send + Sync + ValueEnum + 'static> {
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
        help = "Create a default necessist.toml file in the project's root directory (experimental)"
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
    #[clap(long, help = "Assume testing framework is <FRAMEWORK>")]
    framework: Option<framework::AutoUnion<framework::Identifier, AdditionalIdentifier>>,
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
}

impl<AdditionalIdentifier: Clone + Send + Sync + ValueEnum> From<Opts<AdditionalIdentifier>>
    for (
        Necessist,
        framework::AutoUnion<framework::Identifier, AdditionalIdentifier>,
    )
{
    fn from(opts: Opts<AdditionalIdentifier>) -> Self {
        let Opts {
            allow,
            default_config,
            deny,
            dump,
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
        } = opts;
        let framework = framework.unwrap_or_default();
        let root = root.map(PathBuf::from);
        let test_files = ztest_files.iter().map(PathBuf::from).collect::<Vec<_>>();
        (
            Necessist {
                allow,
                default_config,
                deny,
                dump,
                no_dry_run,
                no_sqlite,
                quiet,
                reset,
                resume,
                root,
                timeout,
                verbose,
                test_files,
            },
            framework,
        )
    }
}
