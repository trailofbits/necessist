use super::super::{TIMEOUT, format_output, path_with_necessist};
use elaborate::std::process::{ChildContext, CommandContext};
use std::{
    env::vars_os,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread::sleep,
    time::{Duration, Instant},
};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) struct Tool {
    pub(crate) name: &'static str,
    tool_path: PathBuf,
    args_fn: fn(&Path, String) -> Vec<String>,
    stdin_null: bool,
}

impl Tool {
    pub(crate) fn claude(tool_path: PathBuf) -> Self {
        Self {
            name: "claude",
            tool_path,
            args_fn: claude_args,
            stdin_null: false,
        }
    }

    pub(crate) fn codex(tool_path: PathBuf) -> Self {
        Self {
            name: "codex",
            tool_path,
            args_fn: codex_args,
            stdin_null: true,
        }
    }

    pub(crate) fn run(&self, skill_dir: &Path, prompt: String) -> Result<Output, String> {
        run_tool_command(self, skill_dir, prompt)
    }
}

fn claude_args(skill_dir: &Path, prompt: String) -> Vec<String> {
    vec![
        format!("--add-dir={}", skill_dir.display()),
        "--allowedTools=Bash,Read".to_owned(),
        "--permission-mode=acceptEdits".to_owned(),
        "--print".to_owned(),
        prompt,
    ]
}

fn codex_args(skill_dir: &Path, prompt: String) -> Vec<String> {
    vec![
        "exec".to_owned(),
        format!("--add-dir={}", skill_dir.to_string_lossy()),
        "--config=sandbox_workspace_write.network_access=false".to_owned(),
        "--config=shell_environment_policy.inherit=\"all\"".to_owned(),
        "--sandbox=workspace-write".to_owned(),
        prompt,
    ]
}

fn run_tool_command(tool: &Tool, skill_dir: &Path, prompt: String) -> Result<Output, String> {
    let mut command = Command::new(&tool.tool_path);
    command
        .args((tool.args_fn)(skill_dir, prompt))
        .env_clear()
        .envs(vars_os())
        .env("PATH", path_with_necessist());

    if tool.stdin_null {
        command.stdin(Stdio::null());
    }

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn_wc()
        .unwrap();
    let start = Instant::now();

    loop {
        if child.try_wait_wc().unwrap().is_some() {
            let output = child.wait_with_output_wc().unwrap();
            if output.status.success() {
                return Ok(output);
            }
            return Err(format!("skill command failed\n{}", format_output(&output)));
        }

        if start.elapsed() >= TIMEOUT {
            drop(child.kill_wc());
            let output = child.wait_with_output_wc().unwrap();
            return Err(format!(
                "skill command timed out after {TIMEOUT:?}\n{}",
                format_output(&output)
            ));
        }

        sleep(POLL_INTERVAL);
    }
}
