#![cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]

use assert_cmd::cargo::cargo_bin_cmd;
use elaborate::std::{
    fs::{create_dir_all_wc, read_to_string_wc, write_wc},
    path::PathContext,
};
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use testing::tempfile_util::tempdir;

#[test]
fn check_skill_warns_about_path_that_is_not_well_known() {
    let home = tempdir().unwrap();
    let skill_path = home.path().join(".claude/skills/necessist-audi/SKILL.md");
    necessist_with_home(&home)
        .arg("--check-skill")
        .arg(&skill_path)
        .assert()
        .code(1)
        .stderr(predicate::eq(format!(
            "Warning: `{}` is not a path that `--find-skill` checks\n",
            skill_path.display()
        )));
}

#[test]
fn check_skill_does_not_warn_about_well_known_path() {
    let home = tempdir().unwrap();
    for subdir in [".claude/skills", ".codex/skills"] {
        let skill_path = skill_path(&home, subdir);
        necessist_with_home(&home)
            .arg("--check-skill")
            .arg(&skill_path)
            .assert()
            .code(1)
            .stderr(predicate::str::is_empty());
    }
}

#[test]
fn check_skill_warns_about_well_known_path_not_under_home() {
    let home = tempdir().unwrap();
    let skill_dir = tempdir().unwrap();
    let skill_path = skill_path(&skill_dir, ".claude/skills");
    necessist_with_home(&home)
        .arg("--check-skill")
        .arg(&skill_path)
        .assert()
        .code(1)
        .stderr(predicate::eq(format!(
            "Warning: `{}` is not a path that `--find-skill` checks\n",
            skill_path.display()
        )));
}

#[test]
fn check_skill_reports_nonexistent_skill() {
    let home = tempdir().unwrap();
    let skill_path = skill_path(&home, ".claude/skills");
    necessist_with_home(&home)
        .arg("--check-skill")
        .arg(&skill_path)
        .assert()
        .code(1)
        .stdout(predicate::eq(format!(
            "skill at `{}` does not exist; pass `--write` to write the current version\n",
            skill_path.display()
        )))
        .stderr(predicate::str::is_empty());
}

#[test]
fn check_skill_with_write_creates_nonexistent_skill() {
    let home = tempdir().unwrap();
    let skill_path = skill_path(&home, ".claude/skills");
    necessist_with_home(&home)
        .args(["--check-skill", skill_path.to_str_wc().unwrap(), "--write"])
        .assert()
        .success()
        .stdout(predicate::eq(format!(
            "skill at `{}` did not exist; written\n",
            skill_path.display()
        )))
        .stderr(predicate::str::is_empty());
    let contents = read_to_string_wc(skill_path).unwrap();
    assert!(contents.lines().any(|line| line == "name: necessist-audit"));
    assert!(
        contents
            .lines()
            .any(|line| line == concat!(r#"  version: ""#, env!("CARGO_PKG_VERSION"), r#"""#))
    );
}

#[test]
fn check_skill_reports_old_skill() {
    let home = tempdir().unwrap();
    let skill_path = skill_path(&home, ".claude/skills");
    create_dir_all_wc(skill_path.parent_wc().unwrap()).unwrap();
    write_wc(&skill_path, skill_with_version("0.0.0")).unwrap();
    necessist_with_home(&home)
        .arg("--check-skill")
        .arg(&skill_path)
        .assert()
        .code(1)
        .stdout(predicate::eq(format!(
            "skill at `{}` is an old version (0.0.0); pass `--write` to update to the current \
             version\n",
            skill_path.display()
        )))
        .stderr(predicate::str::is_empty());
}

#[test]
fn check_skill_with_write_updates_old_skill() {
    let home = tempdir().unwrap();
    let skill_path = skill_path(&home, ".claude/skills");
    create_dir_all_wc(skill_path.parent_wc().unwrap()).unwrap();
    write_wc(&skill_path, skill_with_version("0.0.0")).unwrap();
    necessist_with_home(&home)
        .args(["--check-skill", skill_path.to_str_wc().unwrap(), "--write"])
        .assert()
        .success()
        .stdout(predicate::eq(format!(
            "skill at `{}` was an old version (0.0.0); updated\n",
            skill_path.display()
        )))
        .stderr(predicate::str::is_empty());
    let contents = read_to_string_wc(skill_path).unwrap();
    assert!(
        contents
            .lines()
            .any(|line| line == concat!(r#"  version: ""#, env!("CARGO_PKG_VERSION"), r#"""#))
    );
}

#[test]
fn check_skill_reports_current_skill() {
    let home = tempdir().unwrap();
    let skill_path = skill_path(&home, ".claude/skills");
    create_dir_all_wc(skill_path.parent_wc().unwrap()).unwrap();
    write_wc(&skill_path, skill_with_version(env!("CARGO_PKG_VERSION"))).unwrap();
    necessist_with_home(&home)
        .arg("--check-skill")
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::eq(format!(
            "skill at `{}` is the current version\n",
            skill_path.display()
        )))
        .stderr(predicate::str::is_empty());
}

#[test]
fn check_skill_reports_newer_skill() {
    let home = tempdir().unwrap();
    let skill_path = skill_path(&home, ".claude/skills");
    create_dir_all_wc(skill_path.parent_wc().unwrap()).unwrap();
    write_wc(&skill_path, skill_with_version("999999.0.0")).unwrap();
    necessist_with_home(&home)
        .arg("--check-skill")
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::eq(format!(
            "skill at `{}` is a newer version (999999.0.0); consider updating the `necessist` \
             binary\n",
            skill_path.display()
        )))
        .stderr(predicate::str::is_empty());
}

#[test]
fn check_skill_rejects_unparsable_skill() {
    let home = tempdir().unwrap();
    let skill_path = skill_path(&home, ".claude/skills");
    create_dir_all_wc(skill_path.parent_wc().unwrap()).unwrap();
    write_wc(&skill_path, "no frontmatter here\n").unwrap();
    necessist_with_home(&home)
        .arg("--check-skill")
        .arg(&skill_path)
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::eq(format!(
            r"Error: failed to extract header from `{}`

Caused by:
    failed to find leading `---`
",
            skill_path.display()
        )));
}

#[test]
fn find_skill_checks_well_known_directories() {
    let home = tempdir().unwrap();
    let claude_skill_path = skill_path(&home, ".claude/skills");
    let codex_skill_path = skill_path(&home, ".codex/skills");
    let assert = necessist_with_home(&home)
        .arg("--find-skill")
        .assert()
        .code(1)
        .stderr(predicate::str::is_empty());
    let stdout_normalized = std::str::from_utf8(&assert.get_output().stdout)
        .unwrap()
        .replace('\\', "/");
    let expected_normalized = format!(
        r"skill at `{}` does not exist
skill at `{}` does not exist
",
        claude_skill_path.display(),
        codex_skill_path.display()
    )
    .replace('\\', "/");
    assert_eq!(expected_normalized, stdout_normalized);
}

#[test]
fn find_skill_with_write_updates_old_skills() {
    let home = tempdir().unwrap();
    let claude_skill_path = skill_path(&home, ".claude/skills");
    let codex_skill_path = skill_path(&home, ".codex/skills");
    for skill_path in [&claude_skill_path, &codex_skill_path] {
        create_dir_all_wc(skill_path.parent_wc().unwrap()).unwrap();
        write_wc(skill_path, skill_with_version("0.0.0")).unwrap();
    }
    let assert = necessist_with_home(&home)
        .args(["--find-skill", "--write"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let stdout_normalized = std::str::from_utf8(&assert.get_output().stdout)
        .unwrap()
        .replace('\\', "/");
    let expected_normalized = format!(
        r"skill at `{}` was an old version (0.0.0); updated
skill at `{}` was an old version (0.0.0); updated
",
        claude_skill_path.display(),
        codex_skill_path.display()
    )
    .replace('\\', "/");
    assert_eq!(expected_normalized, stdout_normalized);
    for skill_path in [claude_skill_path, codex_skill_path] {
        let contents = read_to_string_wc(skill_path).unwrap();
        assert!(
            contents
                .lines()
                .any(|line| line == concat!(r#"  version: ""#, env!("CARGO_PKG_VERSION"), r#"""#))
        );
    }
}

#[test]
fn check_skill_and_find_skill_are_incompatible() {
    cargo_bin_cmd!("necessist")
        .args(["--check-skill", "SKILL.md", "--find-skill"])
        .assert()
        .code(2)
        .stderr(predicate::eq(
            "Error: --check-skill and --find-skill are incompatible\n",
        ));
}

#[test]
fn skill_options_and_root_are_incompatible() {
    for (args, stderr) in [
        (
            &["--check-skill", "SKILL.md", "--root", "."] as &[&str],
            "Error: --check-skill and --root are incompatible\n",
        ),
        (
            &["--find-skill", "--root", "."] as &[&str],
            "Error: --find-skill and --root are incompatible\n",
        ),
    ] {
        cargo_bin_cmd!("necessist")
            .args(args)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::eq(stderr));
    }
}

#[test]
fn write_requires_a_skill_option() {
    cargo_bin_cmd!("necessist")
        .arg("--write")
        .assert()
        .code(2)
        .stderr(predicate::eq(
            "Error: --write can be used only with --check-skill <PATH> or --find-skill\n",
        ));
}

/// `necessist` with `HOME` set to `home` so that the paths `--find-skill` checks lie within it.
fn necessist_with_home(home: impl AsRef<Path>) -> assert_cmd::Command {
    let home = home.as_ref();
    let mut command = cargo_bin_cmd!("necessist");
    command.env("HOME", home).env("USERPROFILE", home);
    command
}

/// The path within `home` that `--find-skill` checks for `subdir`, e.g., `.claude/skills`.
fn skill_path(home: impl AsRef<Path>, subdir: &str) -> PathBuf {
    home.as_ref().join(subdir).join("necessist-audit/SKILL.md")
}

fn skill_with_version(version: &str) -> String {
    format!(
        r#"---
name: necessist-audit
metadata:
  version: "{version}"
---

# Test skill
"#
    )
}
