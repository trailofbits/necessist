use assert_cmd::{cargo::cargo_bin, output::OutputError};
use std::{
    env::{join_paths, split_paths, var_os},
    io::{Write, stderr},
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};

mod necessist_audit_support;
use necessist_audit_support::tool::Tool;

const TIMEOUT: Duration = Duration::from_mins(5);

#[test]
#[cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]
fn necessist_audit_update_claude() {
    let Some(tool_path) = command_on_path("claude") else {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping `necessist_audit_update_claude` because `claude` is not on PATH"
        )
        .unwrap();
        return;
    };

    Tool::claude(tool_path).run();
}

#[test]
#[cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]
fn necessist_audit_update_codex() {
    let Some(tool_path) = command_on_path("codex") else {
        #[allow(clippy::explicit_write)]
        writeln!(
            stderr(),
            "Skipping `necessist_audit_update_codex` because `codex` is not on PATH"
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

fn skill_path() -> PathBuf {
    workspace_root().join("core/skills/necessist-audit/SKILL.md")
}

fn path_with_necessist() -> std::ffi::OsString {
    let necessist = cargo_bin("necessist");
    let necessist_dir = necessist.parent().unwrap();
    let paths = std::iter::once(necessist_dir.to_owned()).chain(
        var_os("PATH")
            .map(|path| split_paths(&path).collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    join_paths(paths).unwrap()
}
