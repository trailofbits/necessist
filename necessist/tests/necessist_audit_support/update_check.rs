use super::{
    super::{format_output, skill_path},
    tool::Tool,
};
use std::{
    fs::{create_dir_all, read_to_string, write},
    path::{Path, PathBuf},
    process::Output,
};
use tempfile::TempDir;

#[derive(Clone, Copy)]
pub(crate) enum Scenario {
    Old,
    Current,
    Newer,
}

impl Scenario {
    pub(crate) fn all() -> [Self; 3] {
        [Self::Old, Self::Current, Self::Newer]
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::Current => "current",
            Self::Newer => "newer",
        }
    }

    fn installed_version(self) -> String {
        match self {
            Self::Old => "0.0.0".to_owned(),
            Self::Current => env!("CARGO_PKG_VERSION").to_owned(),
            Self::Newer => "999999.0.0".to_owned(),
        }
    }

    fn expected_output(self, skill: &Path) -> String {
        match self {
            Self::Old => format!(
                "skill at `{}` is an old version (0.0.0); pass `--write` to update to the current \
                 version\n",
                skill.display()
            ),
            Self::Current => format!("skill at `{}` is the current version\n", skill.display()),
            Self::Newer => format!(
                "skill at `{}` is a newer version (999999.0.0); consider updating the `necessist` \
                 binary\n",
                skill.display()
            ),
        }
    }
}

pub(crate) struct SkillCopy {
    _tempdir: TempDir,
    pub(crate) skill: PathBuf,
}

pub(crate) fn prepare_skill_copy(scenario: Scenario) -> SkillCopy {
    let tempdir = TempDir::new().unwrap();
    let current_skill = read_to_string(skill_path()).unwrap();
    let installed_version = scenario.installed_version();
    let installed_skill_contents = replace_skill_version(&current_skill, &installed_version);

    let skill_dir = tempdir.path().join("installed/necessist-audit");
    create_dir_all(&skill_dir).unwrap();

    let skill = skill_dir.join("SKILL.md");
    write(&skill, installed_skill_contents).unwrap();

    SkillCopy {
        _tempdir: tempdir,
        skill,
    }
}

fn replace_skill_version(skill: &str, new_version: &str) -> String {
    skill.replacen(
        concat!("  version: \"", env!("CARGO_PKG_VERSION"), "\""),
        &format!("  version: \"{new_version}\""),
        1,
    )
}

pub(crate) fn assert_update_output(
    tool: &Tool,
    scenario: Scenario,
    skill_copy: &SkillCopy,
    output: &Output,
) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    let expected_output = scenario.expected_output(&skill_copy.skill);

    assert!(
        combined.contains(&expected_output),
        "{} did not relay expected update-check output for `{}`\nexpected: \
         {expected_output}\ninstalled skill: {}\n{}",
        tool.name,
        scenario.name(),
        skill_copy.skill.display(),
        format_output(output)
    );
}
