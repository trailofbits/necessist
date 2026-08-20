use super::{
    super::{format_output, skill_path},
    tool::Tool,
};
use elaborate::std::{
    fs::{create_dir_all_wc, read_to_string_wc, write_wc},
    path::PathContext,
};
use std::{
    path::{Path, PathBuf},
    process::Output,
};
use testing::tempfile_util::{TempDir, tempdir};

type ExpectedOutputFn = fn(&Path) -> String;

pub(crate) struct Test {
    name: &'static str,
    installed_version: &'static str,
    expected_output: ExpectedOutputFn,
}

impl Test {
    pub(crate) fn run(&self, tool: &Tool) {
        let skill_copy = prepare_skill_copy(self.installed_version);
        eprintln!(
            "Running {} update check for `{}` with {}",
            tool.name,
            self.name,
            skill_copy.skill.display()
        );

        let prompt = format!(
            "Use the Necessist audit skill at {} and only check whether the skill is up to date.",
            skill_copy.skill.display()
        );
        let skill_dir = skill_copy.skill.parent_wc().unwrap();
        let output = tool.run(skill_dir, prompt).unwrap_or_else(|error| {
            panic!(
                "{} failed update check for `{}` with {}: {error}",
                tool.name,
                self.name,
                skill_copy.skill.display()
            );
        });
        self.assert_output(tool, &skill_copy, &output);
    }

    fn assert_output(&self, tool: &Tool, skill_copy: &SkillCopy, output: &Output) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");
        let expected_output = (self.expected_output)(&skill_copy.skill);

        assert!(
            combined.contains(&expected_output),
            "{} did not relay expected update-check output for `{}`\nexpected: \
             {expected_output}\ninstalled skill: {}\n{}",
            tool.name,
            self.name,
            skill_copy.skill.display(),
            format_output(output)
        );
    }
}

pub(crate) const TESTS: [Test; 3] = [
    Test {
        name: "old",
        installed_version: "0.0.0",
        expected_output: old_expected_output,
    },
    Test {
        name: "current",
        installed_version: env!("CARGO_PKG_VERSION"),
        expected_output: current_expected_output,
    },
    Test {
        name: "newer",
        installed_version: "999999.0.0",
        expected_output: newer_expected_output,
    },
];

fn old_expected_output(skill: &Path) -> String {
    format!(
        "skill at `{}` is an old version (0.0.0); pass `--write` to update to the current \
         version\n",
        skill.display()
    )
}

fn current_expected_output(skill: &Path) -> String {
    format!("skill at `{}` is the current version\n", skill.display())
}

fn newer_expected_output(skill: &Path) -> String {
    format!(
        "skill at `{}` is a newer version (999999.0.0); consider updating the `necessist` binary\n",
        skill.display()
    )
}

struct SkillCopy {
    _tempdir: TempDir,
    skill: PathBuf,
}

fn prepare_skill_copy(installed_version: &str) -> SkillCopy {
    let tempdir = tempdir().unwrap();
    let current_skill = read_to_string_wc(skill_path()).unwrap();
    let installed_skill_contents = replace_skill_version(&current_skill, installed_version);

    let skill_dir = tempdir.path().join("installed/necessist-audit");
    create_dir_all_wc(&skill_dir).unwrap();

    let skill = skill_dir.join("SKILL.md");
    write_wc(&skill, installed_skill_contents).unwrap();

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
