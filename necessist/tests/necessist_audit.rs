//! A paper on Necessist (Test Harness Mutilation) appeared in Mutation 2024.
//!
//! This test audits one of its Go standard library findings.
//!
//! The structured-report approach is inspired by the worker finding artifacts in
//! the Trail of Bits `c-review` skill:
//! <https://github.com/trailofbits/skills/tree/main/plugins/c-review>

use assert_cmd::output::OutputError;
use necessist_core::util;
use std::{
    env::{split_paths, var_os},
    io::{Write, stderr},
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Duration,
};

mod necessist_audit_support;
use necessist_audit_support::tool::Tool;

const GO_REPO: &str = "https://github.com/golang/go";
const GO_REV: &str = "9a0a82445650eebedf5633fdfe6e73b5836dc5c9";
const JSON_REPORT: &str = "necessist-audit.json";
const OUTPUT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const TIMEOUT: Duration = Duration::from_mins(10);

#[test]
#[cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]
fn necessist_audit_claude() {
    let Some(tool_path) = command_on_path("claude") else {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping `necessist_audit_claude` because `claude` is not on PATH"
        )
        .unwrap();
        return;
    };

    Tool::claude(tool_path).run();
}

#[test]
#[cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]
fn necessist_audit_codex() {
    let Some(tool_path) = command_on_path("codex") else {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping `necessist_audit_codex` because `codex` is not on PATH"
        )
        .unwrap();
        return;
    };

    Tool::codex(tool_path).run();
}

fn command_on_path(tool: &str) -> Option<PathBuf> {
    let path_var = var_os("PATH")?;
    split_paths(&path_var).find_map(|dir| {
        let candidate = dir.join(tool);
        candidate.is_file().then_some(candidate)
    })
}

fn command_output(command: &mut Command) -> Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "command failed\n{}",
        OutputError::new(output.clone())
    );
    output
}

fn format_output(output: &Output) -> String {
    OutputError::new(output.clone()).to_string()
}

#[cfg_attr(dylint_lib = "supplementary", allow(abs_home_path))]
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

fn skill_dir() -> PathBuf {
    workspace_root().join("skills/necessist-audit")
}

fn cache_dir() -> PathBuf {
    workspace_root().join("target/necessist-audit-cache")
}

fn display_path(path: &Path) -> &Path {
    util::strip_prefix(path, &workspace_root()).unwrap_or(path)
}
