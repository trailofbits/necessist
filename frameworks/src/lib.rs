use anyhow::{anyhow, ensure, Error, Result};
use bstr::{io::BufReadExt, BStr};
use clap::ValueEnum;
use heck::ToKebabCase;
use log::debug;
use necessist_core::{
    framework::{Applicable, Interface as High, Postprocess, ToImplementation},
    source_warn, Config, LightContext, Span, WarnFlags, Warning,
};
use std::{
    path::Path,
    process::{Command, Stdio},
};
use strum_macros::EnumIter;
use subprocess::{Exec, NullFile, Redirection};

mod foundry;
use foundry::Foundry;

mod golang;
use golang::Golang;

mod hardhat_ts;
use hardhat_ts::HardhatTs;

mod rust;
use rust::Rust;

mod ts_utils;

#[derive(Debug, Clone, Copy, EnumIter, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[non_exhaustive]
#[remain::sorted]
pub enum Identifier {
    Foundry,
    Golang,
    HardhatTs,
    Rust,
}

impl Applicable for Identifier {
    fn applicable(&self, context: &LightContext) -> Result<bool> {
        match *self {
            Self::Foundry => Foundry::applicable(context),
            Self::Golang => Golang::applicable(context),
            Self::HardhatTs => HardhatTs::applicable(context),
            Self::Rust => Rust::applicable(context),
        }
    }
}

impl ToImplementation for Identifier {
    fn to_implementation(&self, context: &LightContext) -> Result<Option<Box<dyn High>>> {
        Ok(Some(match *self {
            Self::Foundry => implementation_as_interface(Adapter)(Foundry::new(context)),

            Self::Golang => implementation_as_interface(Adapter)(Golang::new(context)),

            // smoelius: `HardhatTs` implements the high-level interface directly.
            Self::HardhatTs => {
                implementation_as_interface(std::convert::identity)(HardhatTs::new(context))
            }

            Self::Rust => implementation_as_interface(Adapter)(Rust::new(context)),
        }))
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_kebab_case())
    }
}

/// Utility function
fn implementation_as_interface<T, U: High + 'static>(
    adapter: impl Fn(T) -> U,
) -> impl Fn(T) -> Box<dyn High> {
    move |implementation| Box::new(adapter(implementation)) as Box<dyn High>
}

#[derive(Debug)]
struct Adapter<T>(T);

impl<T: Low> High for Adapter<T> {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>> {
        self.0.parse(context, config, test_files)
    }

    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()> {
        // smoelius: `REQUIRES_NODE_MODULES` is a hack. But at present, I don't know how it should
        // be generalized.
        if T::REQUIRES_NODE_MODULES {
            ts_utils::install_node_modules(context)?;
        }

        let mut command = self.0.command_to_run_test_file(context, test_file);
        command.args(&context.opts.args);

        debug!("{:?}", command);

        let output = command.output()?;
        ensure!(output.status.success(), "{:#?}", output);
        Ok(())
    }

    fn exec(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        {
            let mut command = self.0.command_to_build_test(context, span);
            command.args(&context.opts.args);
            command.stdout(Stdio::null());
            command.stderr(Stdio::null());

            debug!("{:?}", command);

            let status = command.status()?;
            if !status.success() {
                return Ok(None);
            }
        }

        let (mut command, final_args, init_f_test) = self.0.command_to_run_test(context, span);
        command.args(&context.opts.args);
        command.args(final_args);

        let mut exec = exec_from_command(&command);
        if init_f_test.is_some() {
            exec = exec.stdout(Redirection::Pipe);
        } else {
            exec = exec.stdout(NullFile);
        }
        exec = exec.stderr(NullFile);

        let span = span.clone();

        Ok(Some((
            exec,
            init_f_test.map(|((init, f), test)| -> Box<Postprocess> {
                Box::new(move |context: &LightContext, popen| {
                    let stdout = popen
                        .stdout
                        .as_ref()
                        .ok_or_else(|| anyhow!("Failed to get stdout"))?;
                    let reader = std::io::BufReader::new(stdout);
                    let run = reader.byte_lines().try_fold(init, |prev, result| {
                        let buf = result?;
                        let line = match std::str::from_utf8(&buf) {
                            Ok(line) => line,
                            Err(error) => {
                                source_warn(
                                    context,
                                    Warning::OutputInvalid,
                                    &span,
                                    &format!("{error}: {:?}`", BStr::new(&buf)),
                                    WarnFlags::empty(),
                                )?;
                                return Ok(prev);
                            }
                        };
                        let x = f(line);
                        Ok::<_, Error>(if init { prev && x } else { prev || x })
                    })?;
                    if run {
                        return Ok(true);
                    }
                    source_warn(
                        context,
                        Warning::RunTestFailed,
                        &span,
                        &format!("Failed to run test `{test}`"),
                        WarnFlags::empty(),
                    )?;
                    Ok(false)
                })
            }),
        )))
    }
}

type ProcessLines = (bool, Box<dyn Fn(&str) -> bool>);

trait Low: std::fmt::Debug {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>>;

    const REQUIRES_NODE_MODULES: bool = false;
    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command;
    fn command_to_build_test(&self, context: &LightContext, span: &Span) -> Command;
    fn command_to_run_test(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> (Command, Vec<String>, Option<(ProcessLines, String)>);
}

fn exec_from_command(command: &Command) -> Exec {
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
