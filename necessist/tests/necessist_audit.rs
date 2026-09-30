use assert_cmd::{cargo::cargo_bin, output::OutputError};
use elaborate::std::{
    env::{join_paths_wc, var_os_wc},
    path::PathContext,
};
use std::{
    env::split_paths,
    io::{Write, stderr},
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};

mod necessist_audit_support;
use necessist_audit_support::{tool::Tool, update_check::TESTS};

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

    let tool = Tool::claude(tool_path);
    for test in &TESTS {
        test.run(&tool);
    }
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

    let tool = Tool::codex(tool_path);
    for test in &TESTS {
        test.run(&tool);
    }
}

fn command_on_path(tool: &str) -> Option<PathBuf> {
    let path_var = var_os_wc("PATH").ok()?;
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
        .parent_wc()
        .unwrap()
        .to_owned()
}

fn skill_path() -> PathBuf {
    workspace_root().join("core/skills/necessist-audit/SKILL.md")
}

fn path_with_necessist() -> std::ffi::OsString {
    let necessist = cargo_bin("necessist");
    let necessist_dir = necessist.parent_wc().unwrap();
    let paths = std::iter::once(necessist_dir.to_owned()).chain(
        var_os_wc("PATH")
            .ok()
            .map(|path| split_paths(&path).collect::<Vec<_>>())
            .unwrap_or_default(),
    );
    join_paths_wc(paths).unwrap()
}
