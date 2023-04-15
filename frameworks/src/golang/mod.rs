use super::{ParseLow, ProcessLines, RunLow};
use anyhow::{anyhow, Context, Result};
use necessist_core::{util, warn, Config, LightContext, Span, WarnFlags, Warning};
use std::{collections::BTreeMap, fs::read_to_string, path::Path, process::Command};
use tree_sitter::{Parser, Tree};

mod visitor;
use visitor::visit;

#[derive(Debug)]
pub struct Golang {
    span_test_name_map: BTreeMap<Span, String>,
}

impl Golang {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context.root.join("go.mod").try_exists().map_err(Into::into)
    }

    pub fn new() -> Self {
        Self {
            span_test_name_map: BTreeMap::new(),
        }
    }
}

impl ParseLow for Golang {
    type File = (String, Tree);

    fn check_config(context: &LightContext, config: &Config) -> Result<()> {
        if !config.ignored_functions.is_empty() {
            warn(
                context,
                Warning::IgnoredMacrosUnsupported,
                "The golang framework does not support the `ignored_functions` configuration",
                WarnFlags::ONCE,
            )?;
        }

        if !config.ignored_macros.is_empty() {
            warn(
                context,
                Warning::IgnoredMacrosUnsupported,
                "The golang framework does not support the `ignored_macros` configuration",
                WarnFlags::ONCE,
            )?;
        }

        Ok(())
    }

    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = walkdir::Result<walkdir::DirEntry>>> {
        Box::new(
            walkdir::WalkDir::new(root)
                .into_iter()
                .filter_entry(|entry| {
                    let path = entry.path();
                    !path.is_file() || path.to_string_lossy().ends_with("_test.go")
                }),
        )
    }

    fn parse_file(&self, test_file: &Path) -> Result<Self::File> {
        let text = read_to_string(test_file)?;
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_go::language())
            .with_context(|| "Failed to load Go grammar")?;
        // smoelius: https://github.com/tree-sitter/tree-sitter/issues/255
        parser
            .parse(&text, None)
            .map(|tree| (text, tree))
            .ok_or_else(|| anyhow!("Unspecified error"))
    }

    fn visit(
        &mut self,
        context: &LightContext,
        _config: &Config,
        test_file: &Path,
        file: &Self::File,
    ) -> Result<Vec<Span>> {
        visit(self, context.root.clone(), test_file, &file.0, &file.1)
    }
}

impl RunLow for Golang {
    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command {
        Self::test_command(context, test_file)
    }

    fn command_to_build_test(&self, context: &LightContext, span: &Span) -> Command {
        let mut command = Self::test_command(context, &span.source_file);
        command.arg("-run=^$");
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
            .expect("Test name is not set");

        let mut command = Self::test_command(context, &span.source_file);
        command.args([format!("-run=^{test_name}$").as_ref(), "-v"]);

        let needle = format!("=== RUN   {test_name}");

        (
            command,
            Vec::new(),
            Some((
                (false, Box::new(move |line| line == needle)),
                test_name.clone(),
            )),
        )
    }
}

impl Golang {
    fn test_command(context: &LightContext, test_file: &Path) -> Command {
        #[allow(clippy::expect_used)]
        let package_path = test_file_package_path(context, test_file)
            .expect("Failed to get test file package path");
        let mut command = Command::new("go");
        command.current_dir(context.root.as_path());
        command.arg("test");
        command.arg(package_path);
        command
    }
}

fn test_file_package_path(context: &LightContext, test_file: &Path) -> Result<String> {
    let dir = test_file
        .parent()
        .ok_or_else(|| anyhow!("Failed to get parent"))?;

    let stripped = util::strip_prefix(dir, context.root)?;

    Ok(Path::new(".").join(stripped).to_string_lossy().to_string())
}
