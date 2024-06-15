use super::{rust, ts, OutputAccessors, OutputStrippedOfAnsiScapes, RunHigh};
use anyhow::{anyhow, Error, Result};
use bstr::{io::BufReadExt, BStr};
use log::debug;
use necessist_core::{
    framework::Postprocess, source_warn, util, LightContext, SourceFile, Span, WarnFlags, Warning,
    __Rewriter as Rewriter,
};
use std::{cell::RefCell, env::var, path::Path, process::Command, rc::Rc};
use subprocess::{Exec, NullFile, Redirection};

pub type ProcessLines = (bool, Box<dyn Fn(&str) -> bool>);

pub trait RunLow {
    const REQUIRES_NODE_MODULES: bool = false;
    fn command_to_run_source_file(&self, context: &LightContext, source_file: &Path) -> Command;
    fn instrument_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()>;
    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)>;
    fn command_to_build_file(&self, context: &LightContext, source_file: &Path) -> Command;
    fn command_to_build_test(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> Command;
    fn command_to_run_test(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> (Command, Vec<String>, Option<ProcessLines>);
}

impl<T: RunLow> RunLow for Rc<RefCell<T>> {
    const REQUIRES_NODE_MODULES: bool = T::REQUIRES_NODE_MODULES;
    fn command_to_run_source_file(&self, context: &LightContext, source_file: &Path) -> Command {
        self.borrow()
            .command_to_run_source_file(context, source_file)
    }
    fn instrument_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()> {
        self.borrow()
            .instrument_file(context, rewriter, source_file, n_instrumentable_statements)
    }
    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        self.borrow().statement_prefix_and_suffix(span)
    }
    fn command_to_build_file(&self, context: &LightContext, source_file: &Path) -> Command {
        self.borrow().command_to_build_file(context, source_file)
    }
    fn command_to_build_test(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> Command {
        self.borrow()
            .command_to_build_test(context, test_name, span)
    }
    fn command_to_run_test(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> (Command, Vec<String>, Option<ProcessLines>) {
        self.borrow().command_to_run_test(context, test_name, span)
    }
}

pub struct RunAdapter<T>(pub T);

impl<T: RunLow> RunHigh for RunAdapter<T> {
    fn dry_run(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        // smoelius: `REQUIRES_NODE_MODULES` is a hack. But at present, I don't know how it should
        // be generalized.
        if T::REQUIRES_NODE_MODULES && context.root.join("package.json").try_exists()? {
            ts::utils::install_node_modules(context)?;
        }

        let mut command = self.0.command_to_run_source_file(context, source_file);
        command.args(&context.opts.args);

        debug!("{:?}", command);

        let output = command.output_stripped_of_ansi_escapes()?;
        if !output.status().success() {
            return Err(output.into());
        }
        Ok(())
    }

    fn instrument_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()> {
        self.0
            .instrument_file(context, rewriter, source_file, n_instrumentable_statements)
    }

    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        self.0.statement_prefix_and_suffix(span)
    }

    fn build_file(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        let mut command = self.0.command_to_build_file(context, source_file);
        command.args(&context.opts.args);

        debug!("{:?}", command);

        let output = command.output_stripped_of_ansi_escapes()?;
        if !output.status().success() {
            return Err(output.into());
        }
        Ok(())
    }

    fn exec(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        {
            let mut command = self.0.command_to_build_test(context, test_name, span);
            command.args(&context.opts.args);

            debug!("{:?}", command);

            let output = command.output_stripped_of_ansi_escapes()?;
            if !output.status().success() {
                debug!("{}", output);
                return Ok(None);
            }
        }

        let (mut command, final_args, init_f_test) =
            self.0.command_to_run_test(context, test_name, span);
        command.args(&context.opts.args);
        command.args(final_args);

        let mut exec = util::exec_from_command(&command);
        if init_f_test.is_some() {
            exec = exec.stdout(Redirection::Pipe);
        } else {
            exec = exec.stdout(NullFile);
        }
        exec = exec.stderr(NullFile);

        let test_name = test_name.to_owned();
        let span = span.clone();

        Ok(Some((
            exec,
            init_f_test.map(|(init, f)| -> Box<Postprocess> {
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
                    if enabled("NECESSIST_CHECK_MTIMES") {
                        rust::check_mtimes(context).unwrap();
                    }
                    if run {
                        return Ok(true);
                    }
                    source_warn(
                        context,
                        Warning::RunTestFailed,
                        &span,
                        &format!("Failed to run test `{test_name}`"),
                        WarnFlags::empty(),
                    )?;
                    Ok(false)
                })
            }),
        )))
    }
}

fn enabled(key: &str) -> bool {
    var(key).map_or(false, |value| value != "0")
}
