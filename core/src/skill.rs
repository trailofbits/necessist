use crate::{Necessist, Warning, warn::early_warn};
use anyhow::{Context, Result, bail};
use cargo_util::paths::normalize_path;
use elaborate::std::{
    env::home_dir_wc,
    fs::{create_dir_all_wc, read_to_string_wc, write_wc},
    path::{PathContext, absolute_wc},
};
use semver::Version;
use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
    process::ExitCode,
    str::FromStr,
    sync::LazyLock,
};

const SKILL: &str = include_str!("../skills/necessist-audit/SKILL.md");
const SKILL_SUBPATH: &str = "necessist-audit/SKILL.md";

#[allow(clippy::unwrap_used)]
static SKILL_VERSION: LazyLock<Version> = LazyLock::new(|| {
    let header = skill_header(SKILL).unwrap();
    skill_version(&header).unwrap()
});

/// Which option a check was performed on behalf of. The two differ in how a nonexistent skill is
/// handled: `--check-skill <PATH>` writes it if `--write` was passed, whereas `--find-skill` merely
/// reports it.
#[derive(Clone, Copy)]
enum Mode {
    Check,
    Find,
}

/// The status of a skill *after* a check or find completes.
///
/// So, for a nonexistent skill:
///
/// - If `--check-skill <PATH>` is passed and `--write` is not passed, the skill's status is
///   [`Status::Nonexistent`].
/// - If `--check-skill <PATH>` is passed, `--write` is passed, and the skill is successfully
///   updated, the skill's status is [`Status::Current`].
/// - If `--find-skill` is passed, the skill's status remains [`Status::Nonexistent`], regardless of
///   whether `--write` is passed.
///
/// For an old skill:
///
/// - If `--check-skill <PATH>` or `--find-skill` is passed and `--write` is not passed, the skill's
///   status is [`Status::Old`].
/// - If `--check-skill <PATH>` or `--find-skill` is passed, `--write` is passed, and the skill is
///   successfully updated, the skill's status is [`Status::Current`].
///
/// The status of a current or newer skill is [`Status::Current`] or [`Status::Newer`]
/// (respectively), regardless of whether `--check-skill <PATH>` or `--find-skill` is passed, and
/// regardless of whether `--write` is passed.
///
/// If a skill cannot be updated, an error is returned rather than a status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Nonexistent,
    Old,
    Current,
    Newer,
}

impl Status {
    /// The exit code that this status should return.
    pub fn exit_code(self) -> ExitCode {
        match self {
            Self::Nonexistent | Self::Old => ExitCode::FAILURE,
            Self::Current | Self::Newer => ExitCode::SUCCESS,
        }
    }
}

pub fn check(opts: &Necessist, path: impl AsRef<Path>, write: bool) -> Result<Status> {
    warn_if_not_well_known(opts, &path)?;
    check_impl(path, write)
}

/// Warns if `path` is not a path that `--find-skill` checks. If the home directory cannot be
/// determined, no warning is emitted, as there is nothing to compare `path` to.
fn warn_if_not_well_known(opts: &Necessist, path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let Ok(home) = home_dir_wc() else {
        return Ok(());
    };
    let home_normalized = absolute_normalize_path(&home)?;
    let path_normalized = absolute_normalize_path(path)?;
    if well_known_paths(&home_normalized).any(|well_known_path| well_known_path == path_normalized)
    {
        return Ok(());
    }
    early_warn(
        opts,
        Warning::SkillPathNotWellKnown,
        &format!(
            "`{}` is not a path that `--find-skill` checks",
            path.display()
        ),
    )
}

fn absolute_normalize_path(path: &Path) -> Result<PathBuf> {
    let path_buf = absolute_wc(path)?;
    Ok(normalize_path(&path_buf))
}

fn check_impl(path: impl AsRef<Path>, write: bool) -> Result<Status> {
    check_find_impl(Mode::Check, path, write)
}

pub fn find(write: bool) -> Result<Status> {
    let home = home_dir_wc()?;
    find_impl(&home, write)
}

fn find_impl(home: impl AsRef<Path>, write: bool) -> Result<Status> {
    let home = home.as_ref();
    let mut skill_dirs = Vec::new();
    for skill_dir in well_known_skill_dirs(home) {
        if skill_dir.try_exists_wc()? {
            skill_dirs.push(skill_dir);
        }
    }
    // If no well-known skills directories exist, check them all so that the absence of any
    // installed skill is reported as a failure rather than as a success.
    if skill_dirs.is_empty() {
        skill_dirs.extend(well_known_skill_dirs(home));
    }
    let mut status = Status::Current;
    for skill_dir in skill_dirs {
        let well_known_path_status =
            check_find_impl(Mode::Find, skill_dir.join(SKILL_SUBPATH), write)?;
        // Retain the first status whose exit code is not `ExitCode::SUCCESS`.
        if status.exit_code() == ExitCode::SUCCESS {
            status = well_known_path_status;
        }
    }
    Ok(status)
}

/// The paths that `--find-skill` checks.
fn well_known_paths(home: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    well_known_skill_dirs(home).map(|skill_dir| skill_dir.join(SKILL_SUBPATH))
}

/// The well-known directories in which Necessist looks for skills.
fn well_known_skill_dirs(home: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    const WELL_KNOWN_SUBDIRS: &[&str] = &[".claude/skills", ".codex/skills"];
    WELL_KNOWN_SUBDIRS
        .iter()
        .map(move |subdir| home.as_ref().join(subdir))
}

fn check_find_impl(mode: Mode, path: impl AsRef<Path>, write: bool) -> Result<Status> {
    let path = path.as_ref();
    let path_display = path.display();
    if !path.try_exists_wc()? {
        if matches!(mode, Mode::Find) {
            println!("skill at `{path_display}` does not exist");
            return Ok(Status::Nonexistent);
        }
        let (status, written) = maybe_write_skill(path, write)?;
        let (verb, outcome) = if written {
            ("did", "written")
        } else {
            ("does", "pass `--write` to write the current version")
        };
        println!("skill at `{path_display}` {verb} not exist; {outcome}");
        return Ok(status);
    }
    let contents = read_to_string_wc(path)?;
    let header = skill_header(&contents)
        .with_context(|| format!("failed to extract header from `{path_display}`"))?;
    let Some(name) = skill_name(&header) else {
        bail!("failed to extract skill name from `{path_display}`");
    };
    if name != "necessist-audit" {
        bail!("skill at `{path_display}` is not named `necessist-audit`");
    }
    let version = skill_version(&header)
        .with_context(|| format!("failed to extract version from `{path_display}`"))?;
    match (*SKILL_VERSION).cmp(&version) {
        Ordering::Less => {
            println!(
                "skill at `{path_display}` is a newer version ({version}); consider updating the \
                 `necessist` binary",
            );
            Ok(Status::Newer)
        }
        Ordering::Equal => {
            println!("skill at `{path_display}` is the current version");
            Ok(Status::Current)
        }
        Ordering::Greater => {
            let (status, updated) = maybe_update_skill(path, write)?;
            let (verb, outcome) = if updated {
                ("was", "updated")
            } else {
                ("is", "pass `--write` to update to the current version")
            };
            println!("skill at `{path_display}` {verb} an old version ({version}); {outcome}");
            Ok(status)
        }
    }
}

/// Writes the skill to `path` if `write` is true. Returns the resulting status, along with whether
/// the skill was written. Nothing is printed here so that the caller can emit one complete line,
/// and only once the outcome is known.
fn maybe_write_skill(path: &Path, write: bool) -> Result<(Status, bool)> {
    if write {
        let parent = path.parent_wc()?;
        create_dir_all_wc(parent)?;
        write_wc(path, SKILL)?;
        return Ok((Status::Current, true));
    }
    Ok((Status::Nonexistent, false))
}

fn skill_header(contents: &str) -> Result<yaml_serde::Mapping> {
    let mut lines = contents.lines();
    let Some("---") = lines.next() else {
        bail!("failed to find leading `---`");
    };
    let header_lines = lines.clone().take_while(|&line| line != "---");
    let n = header_lines.clone().count();
    let Some("---") = lines.nth(n) else {
        bail!("failed to find trailing `---`");
    };
    let header_text = header_lines
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    yaml_serde::from_str::<yaml_serde::Mapping>(&header_text).map_err(Into::into)
}

fn skill_name(header: &yaml_serde::Mapping) -> Option<&str> {
    header.get("name").and_then(yaml_serde::Value::as_str)
}

fn skill_version(header: &yaml_serde::Mapping) -> Result<Version> {
    let Some(version) = header
        .get("metadata")
        .and_then(|value| value.as_mapping())
        .and_then(|mapping| mapping.get("version"))
        .and_then(yaml_serde::Value::as_str)
    else {
        bail!("failed to find `metadata.version`");
    };
    Version::from_str(version).map_err(Into::into)
}

/// Like [`maybe_write_skill`], but for a skill that exists and is out of date. The second return
/// value indicates whether the skill was updated.
fn maybe_update_skill(path: &Path, write: bool) -> Result<(Status, bool)> {
    if write {
        write_wc(path, SKILL)?;
        return Ok((Status::Current, true));
    }
    Ok((Status::Old, false))
}

#[cfg(test)]
mod tests {
    #![cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]

    use super::*;
    use testing::tempfile_util::tempdir;

    #[test]
    fn skill_name_is_necessist_audit() {
        let header = skill_header(SKILL).unwrap();
        let name = skill_name(&header).unwrap();
        assert_eq!("necessist-audit", name);
    }

    #[test]
    fn skill_version_is_necessist_version() {
        let necessist_version = Version::from_str(env!("CARGO_PKG_VERSION")).unwrap();
        assert_eq!(necessist_version, *SKILL_VERSION);
    }

    #[test]
    fn skill_header_accepts_markdown_after_frontmatter() {
        let header = skill_header(
            r#"---
name: necessist-audit
metadata:
  version: "1.2.3"
---

---
body"#,
        )
        .unwrap();
        assert_eq!(Some("necessist-audit"), skill_name(&header));
        assert_eq!(Version::new(1, 2, 3), skill_version(&header).unwrap());
    }

    #[test]
    fn skill_header_accepts_crlf() {
        let header = skill_header(
            "---\r\nname: necessist-audit\r\nmetadata:\r\n  version: \"1.2.3\"\r\n---\r\n",
        )
        .unwrap();
        assert_eq!(Some("necessist-audit"), skill_name(&header));
    }

    #[test]
    fn skill_header_rejects_missing_leading_delimiter() {
        let error = skill_header(
            "name: necessist-audit
---",
        )
        .unwrap_err();
        assert_eq!("failed to find leading `---`", error.to_string());
    }

    #[test]
    fn skill_header_rejects_missing_trailing_delimiter() {
        let error = skill_header(
            "---
name: necessist-audit",
        )
        .unwrap_err();
        assert_eq!("failed to find trailing `---`", error.to_string());
    }

    #[test]
    fn skill_header_rejects_malformed_yaml() {
        assert!(
            skill_header(
                "---
name: [
---"
            )
            .is_err()
        );
    }

    #[test]
    fn skill_version_rejects_invalid_version() {
        let header = skill_header(&skill_with_name_and_version(
            "necessist-audit",
            "not-semver",
        ))
        .unwrap();
        assert!(skill_version(&header).is_err());
    }

    #[test]
    fn check_does_not_create_nonexistent_skill() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("nested/SKILL.md");
        assert_eq!(Status::Nonexistent, check_impl(&skill_path, false).unwrap());
        assert!(!skill_path.try_exists_wc().unwrap());
    }

    #[test]
    fn check_with_write_creates_nonexistent_skill() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("nested/SKILL.md");
        assert_eq!(Status::Current, check_impl(&skill_path, true).unwrap());
        assert_eq!(SKILL, read_to_string_wc(skill_path).unwrap());
    }

    #[test]
    fn check_leaves_old_skill_unchanged() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("SKILL.md");
        let contents = skill_with_name_and_version("necessist-audit", "0.0.0");
        write_wc(&skill_path, &contents).unwrap();
        assert_eq!(Status::Old, check_impl(&skill_path, false).unwrap());
        assert_eq!(contents, read_to_string_wc(skill_path).unwrap());
    }

    #[test]
    fn check_with_write_updates_old_skill() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("SKILL.md");
        write_wc(
            &skill_path,
            skill_with_name_and_version("necessist-audit", "0.0.0"),
        )
        .unwrap();
        assert_eq!(Status::Current, check_impl(&skill_path, true).unwrap());
        assert_eq!(SKILL, read_to_string_wc(skill_path).unwrap());
    }

    #[test]
    fn check_with_write_leaves_current_skill_unchanged() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("SKILL.md");
        let contents = skill_with_name_and_version("necessist-audit", &SKILL_VERSION.to_string());
        write_wc(&skill_path, &contents).unwrap();
        assert_eq!(Status::Current, check_impl(&skill_path, true).unwrap());
        assert_eq!(contents, read_to_string_wc(skill_path).unwrap());
    }

    #[test]
    fn check_with_write_leaves_newer_skill_unchanged() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("SKILL.md");
        let contents = skill_with_name_and_version("necessist-audit", "999999.0.0");
        write_wc(&skill_path, &contents).unwrap();
        assert_eq!(Status::Newer, check_impl(&skill_path, true).unwrap());
        assert_eq!(contents, read_to_string_wc(skill_path).unwrap());
    }

    #[test]
    fn check_rejects_malformed_header() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("SKILL.md");
        write_wc(&skill_path, "no frontmatter here\n").unwrap();
        let error = format!("{:#}", check_impl(&skill_path, false).unwrap_err());
        assert_eq!(
            format!(
                "failed to extract header from `{}`: failed to find leading `---`",
                skill_path.display()
            ),
            error
        );
    }

    #[test]
    fn check_rejects_missing_name() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("SKILL.md");
        write_wc(
            &skill_path,
            r#"---
metadata:
  version: "1.0.0"
---
"#,
        )
        .unwrap();
        let error = check_impl(&skill_path, false).unwrap_err().to_string();
        assert_eq!(
            format!(
                "failed to extract skill name from `{}`",
                skill_path.display()
            ),
            error
        );
    }

    #[test]
    fn check_rejects_wrong_name() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("SKILL.md");
        write_wc(
            &skill_path,
            skill_with_name_and_version("another-skill", "1.0.0"),
        )
        .unwrap();
        let error = check_impl(&skill_path, false).unwrap_err().to_string();
        assert_eq!(
            format!(
                "skill at `{}` is not named `necessist-audit`",
                skill_path.display()
            ),
            error
        );
    }

    #[test]
    fn check_rejects_missing_version() {
        let skill_dir = tempdir().unwrap();
        let skill_path = skill_dir.path().join("SKILL.md");
        write_wc(
            &skill_path,
            r"---
name: necessist-audit
---
",
        )
        .unwrap();
        let error = format!("{:#}", check_impl(&skill_path, false).unwrap_err());
        assert_eq!(
            format!(
                "failed to extract version from `{}`: failed to find `metadata.version`",
                skill_path.display()
            ),
            error
        );
    }

    #[test]
    fn find_updates_old_skill_but_does_not_create_nonexistent_skill() {
        let home = tempdir().unwrap();
        let [claude_skill_path, codex_skill_path] = well_known_paths(&home)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        create_dir_all_wc(claude_skill_path.parent_wc().unwrap()).unwrap();
        write_wc(
            &claude_skill_path,
            skill_with_name_and_version("necessist-audit", "0.0.0"),
        )
        .unwrap();
        create_dir_all_wc(home.path().join(".codex/skills")).unwrap();

        assert_eq!(Status::Nonexistent, find_impl(&home, true).unwrap());

        assert_eq!(SKILL, read_to_string_wc(claude_skill_path).unwrap());
        assert!(!codex_skill_path.try_exists_wc().unwrap());
    }

    fn skill_with_name_and_version(name: &str, version: &str) -> String {
        format!(
            r#"---
name: {name}
metadata:
  version: "{version}"
---

# Test skill
"#
        )
    }
}
