use super::{OutputAccessors, OutputStrippedOfAnsiScapes, RunHigh, rust};
use anyhow::{Error, Result, anyhow};
use assert_cmd::output::OutputError;
use bstr::{BStr, io::BufReadExt};
use log::debug;
use necessist_core::{
    __Rewriter as Rewriter, LightContext, SourceFile, Span, WarnFlags, Warning,
    framework::Postprocess, source_warn, util,
};
use std::{
    cell::RefCell,
    env::var,
    fs::File,
    io::{Read, Seek},
    path::Path,
    process::{Command, ExitStatus as StdExitStatus, Output},
    rc::Rc,
};
use subprocess::{Exec, Redirection};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;

pub type ProcessLines = (bool, Box<dyn Fn(&str) -> bool>);

pub trait RunLow {
    fn install_dependencies(&self, _context: &LightContext) -> Result<()> {
        Ok(())
    }
    fn command_to_run_source_file(&self, context: &LightContext, source_file: &Path) -> Command;
    fn instrument_source_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()>;
    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)>;
    fn command_to_build_source_file(&self, context: &LightContext, source_file: &Path) -> Command;
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
    fn install_dependencies(&self, context: &LightContext) -> Result<()> {
        self.borrow().install_dependencies(context)
    }
    fn command_to_run_source_file(&self, context: &LightContext, source_file: &Path) -> Command {
        self.borrow()
            .command_to_run_source_file(context, source_file)
    }
    fn instrument_source_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()> {
        self.borrow().instrument_source_file(
            context,
            rewriter,
            source_file,
            n_instrumentable_statements,
        )
    }
    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        self.borrow().statement_prefix_and_suffix(span)
    }
    fn command_to_build_source_file(&self, context: &LightContext, source_file: &Path) -> Command {
        self.borrow()
            .command_to_build_source_file(context, source_file)
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

/// Implements [`RunHigh`] for `T`, given that `T` implements [`RunLow`]
pub struct RunAdapter<T>(pub T);

impl<T: RunLow> RunHigh for RunAdapter<T> {
    fn dry_run(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        self.0.install_dependencies(context)?;

        let mut command = self.0.command_to_run_source_file(context, source_file);
        command.args(&context.opts.args);

        debug!("{command:?}");

        let output = command.output_stripped_of_ansi_escapes()?;
        if !output.status().success() {
            return Err(output.into());
        }
        Ok(())
    }

    fn instrument_source_file(
        &self,
        context: &LightContext,
        rewriter: &mut Rewriter,
        source_file: &SourceFile,
        n_instrumentable_statements: usize,
    ) -> Result<()> {
        self.0
            .instrument_source_file(context, rewriter, source_file, n_instrumentable_statements)
    }

    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        self.0.statement_prefix_and_suffix(span)
    }

    fn build_source_file(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        let mut command = self.0.command_to_build_source_file(context, source_file);
        command.args(&context.opts.args);

        debug!("{command:?}");

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
    ) -> Result<Result<(Exec, Option<Box<Postprocess>>)>> {
        {
            let mut command = self.0.command_to_build_test(context, test_name, span);
            command.args(&context.opts.args);

            debug!("{command:?}");

            let output = command.output_stripped_of_ansi_escapes()?;
            if !output.status().success() {
                return Ok(Err(output.into()));
            }
        }

        let (mut command, final_args, init_f_test) =
            self.0.command_to_run_test(context, test_name, span);
        command.args(&context.opts.args);
        command.args(final_args);

        let test_name = test_name.to_owned();
        let span = span.clone();

        let mut exec = util::exec_from_command(&command);
        let postprocess = if let Some((init, f)) = init_f_test {
            // `perform_exec` waits for the child before postprocessing its output. Spool to files
            // so a child producing more than a pipe buffer can still exit.
            let stdout_file = tempfile::tempfile()?;
            let stderr_file = tempfile::tempfile()?;
            exec = exec.stdout(Redirection::File(stdout_file.try_clone()?));
            exec = exec.stderr(Redirection::File(stderr_file.try_clone()?));
            // `Postprocess` is an `Fn`, even though it is called only once. Use interior mutability
            // to consume the captured files when that call occurs.
            let stdout_file = RefCell::new(Some(stdout_file));
            let stderr_file = RefCell::new(Some(stderr_file));
            Some({
                let postprocess: Box<Postprocess> = Box::new(move |context, popen| {
                    let mut stdout_file = stdout_file
                        .borrow_mut()
                        .take()
                        .ok_or_else(|| anyhow!("Failed to get stdout"))?;
                    stdout_file.rewind()?;
                    let stdout = read_file_to_end(stdout_file)?;
                    let run = stdout.byte_lines().try_fold(init, |prev, result| {
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
                    let mut stderr_file = stderr_file
                        .borrow_mut()
                        .take()
                        .ok_or_else(|| anyhow!("Failed to get stderr"))?;
                    stderr_file.rewind()?;
                    let stderr = read_file_to_end(stderr_file)?;
                    let status = popen.wait()?;
                    let Some(code) = status.code() else {
                        return Err(anyhow!("Unexpected exit status: {status:?}"));
                    };
                    // smoelius: `raw` is `i32` on Unix, and `u32` on Windows.
                    let raw = code.try_into()?;
                    let error = OutputError::new(Output {
                        status: StdExitStatus::from_raw(raw),
                        stdout,
                        stderr,
                    });
                    source_warn(
                        context,
                        Warning::RunTestFailed,
                        &span,
                        &format!("Failed to run test `{test_name}`: {error}"),
                        WarnFlags::empty(),
                    )?;
                    Ok(false)
                });
                postprocess
            })
        } else {
            exec = exec.stdout(Redirection::Null);
            exec = exec.stderr(Redirection::Null);
            None
        };

        Ok(Ok((exec, postprocess)))
    }
}

fn read_file_to_end(mut file: File) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let _: usize = file.read_to_end(&mut buf)?;
    Ok(buf)
}

fn enabled(key: &str) -> bool {
    var(key).is_ok_and(|value| value != "0")
}
