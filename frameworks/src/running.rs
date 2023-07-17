use super::{ts, utils, RunHigh};
use anyhow::{anyhow, Context, Error, Result};
use assert_cmd::output::OutputError;
use bstr::{io::BufReadExt, BStr};
use log::debug;
use necessist_core::{framework::Postprocess, source_warn, LightContext, Span, WarnFlags, Warning};
use std::{cell::RefCell, path::Path, process::Command, rc::Rc};
use subprocess::{Exec, NullFile, Redirection};

pub type ProcessLines = (bool, Box<dyn Fn(&str) -> bool>);

pub trait RunLow {
    const REQUIRES_NODE_MODULES: bool = false;
    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command;
    fn command_to_build_test(&self, context: &LightContext, span: &Span) -> Command;
    fn command_to_run_test(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> (Command, Vec<String>, Option<(ProcessLines, String)>);
}

impl<T: RunLow> RunLow for Rc<RefCell<T>> {
    const REQUIRES_NODE_MODULES: bool = T::REQUIRES_NODE_MODULES;
    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command {
        self.borrow().command_to_run_test_file(context, test_file)
    }
    fn command_to_build_test(&self, context: &LightContext, span: &Span) -> Command {
        self.borrow().command_to_build_test(context, span)
    }
    fn command_to_run_test(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> (Command, Vec<String>, Option<(ProcessLines, String)>) {
        self.borrow().command_to_run_test(context, span)
    }
}

pub struct RunAdapter<T>(pub T);

impl<T: RunLow> RunHigh for RunAdapter<T> {
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()> {
        // smoelius: `REQUIRES_NODE_MODULES` is a hack. But at present, I don't know how it should
        // be generalized.
        if T::REQUIRES_NODE_MODULES && context.root.join("package.json").try_exists()? {
            ts::utils::install_node_modules(context)?;
        }

        let mut command = self.0.command_to_run_test_file(context, test_file);
        command.args(&context.opts.args);

        debug!("{:?}", command);

        let output = command
            .output()
            .with_context(|| format!("Failed to run command: {command:?}"))?;
        if !output.status.success() {
            return Err(OutputError::new(output).into());
        }
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

            debug!("{:?}", command);

            let output = command.output()?;
            if !output.status.success() {
                debug!("{}", OutputError::new(output));
                return Ok(None);
            }
        }

        let (mut command, final_args, init_f_test) = self.0.command_to_run_test(context, span);
        command.args(&context.opts.args);
        command.args(final_args);

        let mut exec = utils::exec_from_command(&command);
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
