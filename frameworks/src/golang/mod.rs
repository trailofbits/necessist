use super::{Low, ProcessLines};
use anyhow::{anyhow, Context, Result};
use necessist_core::{util, warn, Config, LightContext, Span, WarnFlags, Warning};
use std::{
    collections::BTreeMap,
    fs::read_to_string,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
};
use tree_sitter::Parser;
use walkdir::WalkDir;

mod visitor;
use visitor::visit;

#[derive(Debug)]
pub struct Golang {
    root: Rc<PathBuf>,
    span_test_name_map: BTreeMap<Span, String>,
}

impl Golang {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context.root.join("go.mod").try_exists().map_err(Into::into)
    }

    pub fn new(context: &LightContext) -> Self {
        Self {
            root: Rc::new(context.root.to_path_buf()),
            span_test_name_map: BTreeMap::new(),
        }
    }
}

impl Low for Golang {
    #[cfg_attr(
        dylint_lib = "non_local_effect_before_error_return",
        allow(non_local_effect_before_error_return)
    )]
    fn parse(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>> {
        check_config(context, config)?;

        let mut spans = Vec::new();

        let mut visit_test_file = |test_file: &Path| -> Result<()> {
            assert!(test_file.is_absolute());
            assert!(test_file.starts_with(context.root));
            let text = read_to_string(test_file)?;
            let mut parser = Parser::new();
            parser
                .set_language(tree_sitter_go::language())
                .with_context(|| "Failed to load Go grammar")?;
            #[allow(clippy::unwrap_used)]
            let tree = parser.parse(&text, None).ok_or_else(|| {
                anyhow!(
                    "Failed to parse {:?}",
                    util::strip_prefix(test_file, context.root).unwrap()
                )
            })?;
            let spans_visited = visit(self, self.root.clone(), test_file, &text, &tree)?;
            spans.extend(spans_visited);
            Ok(())
        };

        if test_files.is_empty() {
            for entry in WalkDir::new(context.root) {
                let entry = entry?;
                let path = entry.path();

                if !path.to_string_lossy().ends_with("_test.go") {
                    continue;
                }

                visit_test_file(path)?;
            }
        } else {
            for path in test_files {
                visit_test_file(path)?;
            }
        }

        Ok(spans)
    }

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
        command.current_dir(context.root);
        command.arg("test");
        command.arg(package_path);
        command
    }
}

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

fn test_file_package_path(context: &LightContext, test_file: &Path) -> Result<String> {
    let dir = test_file
        .parent()
        .ok_or_else(|| anyhow!("Failed to get parent"))?;

    let stripped = util::strip_prefix(dir, context.root)?;

    Ok(Path::new(".").join(stripped).to_string_lossy().to_string())
}
