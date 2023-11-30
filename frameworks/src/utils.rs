use anyhow::{Context, Result};
use assert_cmd::output::OutputError;
use std::process::{Command, ExitStatus, Output};
use subprocess::Exec;

pub fn exec_from_command(command: &Command) -> Exec {
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
