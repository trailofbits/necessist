use super::{ParseLow, ProcessLines, RunLow};
use anyhow::{anyhow, Result};
use necessist_core::{util, warn, Config, LightContext, Span, WarnFlags, Warning};
use solang_parser::pt::SourceUnit;
use std::{collections::BTreeMap, fs::read_to_string, path::Path, process::Command};

mod visitor;
use visitor::visit;

#[derive(Debug)]
pub struct Foundry {
    span_test_name_map: BTreeMap<Span, String>,
}

impl Foundry {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context
            .root
            .join("foundry.toml")
            .try_exists()
            .map_err(Into::into)
    }

    pub fn new() -> Self {
        Self {
            span_test_name_map: BTreeMap::new(),
        }
    }
}

impl ParseLow for Foundry {
    type File = (String, SourceUnit);

    fn check_config(context: &LightContext, config: &Config) -> Result<()> {
        if !config.ignored_functions.is_empty() {
            warn(
                context,
                Warning::IgnoredMacrosUnsupported,
                "The foundry framework does not support the `ignored_functions` configuration",
                WarnFlags::ONCE,
            )?;
        }

        if !config.ignored_macros.is_empty() {
            warn(
                context,
                Warning::IgnoredMacrosUnsupported,
                "The foundry framework does not support the `ignored_macros` configuration",
                WarnFlags::ONCE,
            )?;
        }

        Ok(())
    }

    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = walkdir::Result<walkdir::DirEntry>>> {
        Box::new(
            walkdir::WalkDir::new(root.join("test"))
                .into_iter()
                .filter_entry(|entry| {
                    let path = entry.path();
                    !path.is_file() || path.to_string_lossy().ends_with(".t.sol")
                }),
        )
    }

    fn parse_file(&self, test_file: &Path) -> Result<Self::File> {
        let contents = read_to_string(test_file)?;
        solang_parser::parse(&contents, 0)
            .map(|(source_unit, _)| (contents, source_unit))
            .map_err(|error| anyhow!(format!("{error:?}")))
    }

    fn visit(
        &mut self,
        context: &LightContext,
        _config: &Config,
        test_file: &Path,
        file: &Self::File,
    ) -> Result<Vec<Span>> {
        Ok(visit(
            self,
            context.root.clone(),
            test_file,
            &file.0,
            &file.1,
        ))
    }
}

impl RunLow for Foundry {
    const REQUIRES_NODE_MODULES: bool = true;

    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command {
        Self::test_command(context, test_file)
    }

    fn command_to_build_test(&self, context: &LightContext, _span: &Span) -> Command {
        let mut command = Command::new("forge");
        command.current_dir(context.root.as_path());
        command.arg("build");
        command
    }

    fn command_to_run_test(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> (Command, Vec<String>, Option<(ProcessLines, String)>) {
        #[allow(clippy::expect_used)]
        let test_name = self
            .span_test_name_map
            .get(span)
            .cloned()
            .expect("Test ident is not set");

        let mut command = Self::test_command(context, &span.source_file);
        command.args(["--match-test", &test_name]);

        (
            command,
            Vec::new(),
            Some((
                (
                    true,
                    Box::new(|line| !line.starts_with("No tests match the provided pattern")),
                ),
                test_name,
            )),
        )
    }
}

impl Foundry {
    fn test_command(context: &LightContext, test_file: &Path) -> Command {
        let mut command = Command::new("forge");
        command.current_dir(context.root.as_path());
        command.env("FOUNDRY_FUZZ_RUNS", "1");
        command.args([
            "test",
            "--match-path",
            &util::strip_prefix(test_file, context.root)
                .unwrap()
                .to_string_lossy(),
        ]);
        command
    }

    fn set_span_test_name(&mut self, span: &Span, name: &str) {
        self.span_test_name_map
            .insert(span.clone(), name.to_owned());
    }
}
