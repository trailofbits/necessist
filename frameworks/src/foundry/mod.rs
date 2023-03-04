use super::Low;
use anyhow::{anyhow, Context, Result};
use necessist_core::{
    framework::Postprocess, source_warn, util, warn, Config, LightContext, Span, WarnFlags, Warning,
};
use std::{
    collections::BTreeMap,
    fs::read_to_string,
    io::BufRead,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
};
use walkdir::WalkDir;

mod visit;

mod visitor;
use visitor::visit;

#[derive(Debug)]
pub struct Foundry {
    root: Rc<PathBuf>,
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

    pub fn new(context: &LightContext) -> Self {
        Self {
            root: Rc::new(context.root.to_path_buf()),
            span_test_name_map: BTreeMap::new(),
        }
    }
}

impl Low for Foundry {
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
            let contents = read_to_string(test_file)?;
            #[allow(clippy::unwrap_used)]
            let (mut source_unit, _comments) = solang_parser::parse(&contents, 0)
                .map_err(|error| anyhow!(format!("{error:?}")))
                .with_context(|| {
                    format!(
                        "Failed to parse {:?}",
                        util::strip_prefix(test_file, context.root).unwrap()
                    )
                })?;
            let spans_visited = visit(
                self,
                self.root.clone(),
                test_file,
                &contents,
                &mut source_unit,
            );
            spans.extend(spans_visited);
            Ok(())
        };

        if test_files.is_empty() {
            for entry in WalkDir::new(context.root.join("test")) {
                let entry = entry?;
                let path = entry.path();

                if !path.to_string_lossy().ends_with(".t.sol") {
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

    const REQUIRES_NODE_MODULES: bool = true;

    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command {
        Self::test_command(context, test_file)
    }

    fn command_to_build_test(&self, context: &LightContext, _span: &Span) -> Command {
        let mut command = Command::new("forge");
        command.current_dir(context.root);
        command.arg("build");
        command
    }

    fn command_to_run_test(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> (Command, Vec<String>, Option<Box<Postprocess>>) {
        #[allow(clippy::expect_used)]
        let test_name = self
            .span_test_name_map
            .get(span)
            .cloned()
            .expect("Test ident is not set");

        let mut command = Self::test_command(context, &span.source_file);
        command.args(["--match-test", &test_name]);

        let span = span.clone();

        (
            command,
            Vec::new(),
            Some(Box::new(move |context: &LightContext, popen| {
                let stdout = popen
                    .stdout
                    .as_ref()
                    .ok_or_else(|| anyhow!("Failed to get stdout"))?;
                let reader = std::io::BufReader::new(stdout);
                let no_tests_matched = reader.lines().try_fold(false, |prev, line| {
                    let line = line?;
                    Ok::<_, std::io::Error>(
                        prev || line.starts_with("No tests match the provided pattern"),
                    )
                })?;
                if no_tests_matched {
                    source_warn(
                        context,
                        Warning::RunTestFailed,
                        &span,
                        &format!("Failed to run test `{test_name}`"),
                        WarnFlags::empty(),
                    )?;
                    Ok(false)
                } else {
                    Ok(true)
                }
            })),
        )
    }
}

impl Foundry {
    fn test_command(context: &LightContext, test_file: &Path) -> Command {
        let mut command = Command::new("forge");
        command.current_dir(context.root);
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
