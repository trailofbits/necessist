use anyhow::{Context, Result};
use assert_cmd::output::OutputError;
use std::process::{Command, ExitStatus, Output};

pub trait OutputStrippedOfAnsiScapes {
    fn output_stripped_of_ansi_escapes(&mut self) -> Result<OutputError>;
}

impl OutputStrippedOfAnsiScapes for Command {
    fn output_stripped_of_ansi_escapes(&mut self) -> Result<OutputError> {
        #[allow(clippy::disallowed_methods)]
        let Output {
            status,
            stdout,
            stderr,
        } = self
            .output()
            .with_context(|| format!("Failed to run command: {self:?}"))?;
        Ok(OutputError::new(Output {
            status,
            stdout: strip_ansi_escapes::strip(stdout),
            stderr: strip_ansi_escapes::strip(stderr),
        }))
    }
}

// smoelius: The `stderr` method is currently unused.
#[allow(dead_code)]
pub trait OutputAccessors {
    fn status(&self) -> ExitStatus;
    fn stdout(&self) -> &[u8];
    fn stderr(&self) -> &[u8];
}

impl OutputAccessors for OutputError {
    fn status(&self) -> ExitStatus {
        self.as_output().unwrap().status
    }
    fn stdout(&self) -> &[u8] {
        &self.as_output().unwrap().stdout
    }
    fn stderr(&self) -> &[u8] {
        &self.as_output().unwrap().stderr
    }
}
