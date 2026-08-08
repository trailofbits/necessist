use anyhow::{Result, bail};
use semver::Version;
use std::{cmp::Ordering, fs, path::Path, str::FromStr, sync::LazyLock};

const SKILL: &str = include_str!("../skills/necessist-audit/SKILL.md");

#[allow(clippy::unwrap_used)]
static SKILL_VERSION: LazyLock<Version> = LazyLock::new(|| {
    let header = skill_header(SKILL).unwrap();
    skill_version(&header).unwrap()
});

pub fn check(path: impl AsRef<Path>, write: bool) -> Result<()> {
    let path = path.as_ref();
    let path_display = path.display();
    if !path.try_exists()? {
        print!("skill at `{path_display}` does not exist; ");
        return maybe_write_skill(path, write);
    }
    let contents = fs::read_to_string(path)?;
    let header = skill_header(&contents)?;
    let Some(name) = skill_name(&header) else {
        bail!("failed to get skill name from `{path_display}`");
    };
    if name != "necessist-audit" {
        bail!("skill name is not `necessist-audit`");
    }
    let version = skill_version(&header)?;
    match (*SKILL_VERSION).cmp(&version) {
        Ordering::Less => {
            println!(
                "skill at `{path_display}` is a newer version ({version}); consider updating the \
                 `necessist` binary",
            );
        }
        Ordering::Equal => {
            println!("skill at `{path_display}` is the current version");
        }
        Ordering::Greater => {
            print!("skill at `{path_display}` is an old version ({version}); ");
            maybe_update_skill(path, write)?;
        }
    }
    Ok(())
}

fn maybe_write_skill(path: &Path, write: bool) -> Result<()> {
    println!(
        "{}",
        if write {
            "writing"
        } else {
            "pass `--write` to write the current version"
        }
    );
    if write {
        let Some(parent) = path.parent() else {
            bail!("failed to get parent directory");
        };
        fs::create_dir_all(parent)?;
        fs::write(path, SKILL)?;
    }
    Ok(())
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
        bail!("failed to get skill version");
    };
    Version::from_str(version).map_err(Into::into)
}

fn maybe_update_skill(path: &Path, write: bool) -> Result<()> {
    println!(
        "{}",
        if write {
            "updating"
        } else {
            "pass `--write` to update to the current version"
        }
    );
    if write {
        fs::write(path, SKILL)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![cfg_attr(dylint_lib = "general", allow(non_thread_safe_call_in_test))]

    use super::*;
    use testing::tempfile_util::tempdir;

    fn skill_with(name: &str, version: &str) -> String {
        format!("---\nname: {name}\nmetadata:\n  version: \"{version}\"\n---\n\n# Test skill\n")
    }

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
            "---\nname: necessist-audit\nmetadata:\n  version: \"1.2.3\"\n---\n\n---\nbody",
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
        let error = skill_header("name: necessist-audit\n---").unwrap_err();
        assert!(error.to_string().contains("failed to find leading `---`"));
    }

    #[test]
    fn skill_header_rejects_missing_trailing_delimiter() {
        let error = skill_header("---\nname: necessist-audit").unwrap_err();
        assert!(error.to_string().contains("failed to find trailing `---`"));
    }

    #[test]
    fn skill_header_rejects_malformed_yaml() {
        assert!(skill_header("---\nname: [\n---").is_err());
    }

    #[test]
    fn check_missing_skill_without_write_does_not_create_file() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("nested/SKILL.md");
        check(&path_buf, false).unwrap();
        assert!(!path_buf.try_exists().unwrap());
    }

    #[test]
    fn check_missing_skill_with_write_creates_parent_dirs_and_exact_contents() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("nested/SKILL.md");
        check(&path_buf, true).unwrap();
        assert_eq!(SKILL, fs::read_to_string(path_buf).unwrap());
    }

    #[test]
    fn check_outdated_skill_without_write_leaves_file_unchanged() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("SKILL.md");
        let contents = skill_with("necessist-audit", "0.0.0");
        fs::write(&path_buf, &contents).unwrap();
        check(&path_buf, false).unwrap();
        assert_eq!(contents, fs::read_to_string(path_buf).unwrap());
    }

    #[test]
    fn check_outdated_skill_with_write_replaces_file() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("SKILL.md");
        fs::write(&path_buf, skill_with("necessist-audit", "0.0.0")).unwrap();
        check(&path_buf, true).unwrap();
        assert_eq!(SKILL, fs::read_to_string(path_buf).unwrap());
    }

    #[test]
    fn check_current_skill_leaves_file_unchanged() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("SKILL.md");
        let contents = skill_with("necessist-audit", &SKILL_VERSION.to_string());
        fs::write(&path_buf, &contents).unwrap();
        check(&path_buf, true).unwrap();
        assert_eq!(contents, fs::read_to_string(path_buf).unwrap());
    }

    #[test]
    fn check_newer_skill_is_not_downgraded() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("SKILL.md");
        let contents = skill_with("necessist-audit", "999999.0.0");
        fs::write(&path_buf, &contents).unwrap();
        check(&path_buf, true).unwrap();
        assert_eq!(contents, fs::read_to_string(path_buf).unwrap());
    }

    #[test]
    fn check_rejects_missing_name() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("SKILL.md");
        fs::write(&path_buf, "---\nmetadata:\n  version: \"1.0.0\"\n---\n").unwrap();
        assert!(
            check(&path_buf, false)
                .unwrap_err()
                .to_string()
                .contains("failed to get skill name")
        );
    }

    #[test]
    fn check_rejects_wrong_name() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("SKILL.md");
        fs::write(&path_buf, skill_with("another-skill", "1.0.0")).unwrap();
        assert!(
            check(&path_buf, false)
                .unwrap_err()
                .to_string()
                .contains("skill name is not")
        );
    }

    #[test]
    fn check_rejects_missing_version() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("SKILL.md");
        fs::write(&path_buf, "---\nname: necessist-audit\n---\n").unwrap();
        assert!(
            check(&path_buf, false)
                .unwrap_err()
                .to_string()
                .contains("failed to get skill version")
        );
    }

    #[test]
    fn check_rejects_non_string_version() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("SKILL.md");
        fs::write(
            &path_buf,
            "---\nname: necessist-audit\nmetadata:\n  version: 1\n---\n",
        )
        .unwrap();
        assert!(
            check(&path_buf, false)
                .unwrap_err()
                .to_string()
                .contains("failed to get skill version")
        );
    }

    #[test]
    fn check_rejects_invalid_version() {
        let tempdir = tempdir().unwrap();
        let path_buf = tempdir.path().join("SKILL.md");
        fs::write(&path_buf, skill_with("necessist-audit", "not-semver")).unwrap();
        assert!(check(&path_buf, false).is_err());
    }
}
