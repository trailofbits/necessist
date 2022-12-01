use anyhow::{anyhow, ensure, Context, Result};
use log::debug;
use necessist_core::{
    source_warn, util, warn, Config, Interface, LightContext, Postprocess, Span, WarnFlags, Warning,
};
use std::{
    collections::BTreeMap,
    fs::read_to_string,
    io::BufRead,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
};
use subprocess::{Exec, NullFile, Redirection};
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
    pub fn applicable(context: &LightContext) -> Result<Option<Self>> {
        if context.root.join("foundry.toml").try_exists()? {
            Ok(Some(Self::new(context)))
        } else {
            Ok(None)
        }
    }

    fn new(context: &LightContext) -> Self {
        Self {
            root: Rc::new(context.root.to_path_buf()),
            span_test_name_map: BTreeMap::new(),
        }
    }
}

impl Interface for Foundry {
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
            let (mut source_unit, _comments) = solang_parser::parse(&contents, 0)
                .map_err(|error| anyhow!(format!("{:?}", error)))
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

    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()> {
        let mut command = Command::new("forge");
        command.current_dir(context.root);
        command.env("FOUNDRY_FUZZ_RUNS", "1");
        command.args(["test", "--match-path", &test_file.to_string_lossy()]);

        debug!("{:?}", command);

        let output = command.output()?;
        ensure!(output.status.success(), "{:#?}", output);
        Ok(())
    }

    fn exec(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        #[allow(clippy::expect_used)]
        let test_name = self
            .span_test_name_map
            .get(span)
            .cloned()
            .expect("Test ident is not set");

        {
            let mut command = Command::new("forge");
            command.current_dir(context.root);
            command.arg("build");
            command.stdout(Stdio::null());
            command.stderr(Stdio::null());

            debug!("{:?}", command);

            let status = command.status()?;
            if !status.success() {
                return Ok(None);
            }
        }

        let mut exec = Exec::cmd("forge");
        exec = exec.cwd(context.root);
        exec = exec.env("FOUNDRY_FUZZ_RUNS", "1");
        exec = exec.args(&[
            "test",
            "--match-path",
            &util::strip_prefix(&span.source_file, context.root)
                .unwrap()
                .to_string_lossy(),
            "--match-test",
            &test_name,
        ]);
        exec = exec.stdout(Redirection::Pipe);
        exec = exec.stderr(NullFile);

        let span = span.clone();

        Ok(Some((
            exec,
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
                        &format!("Failed to run test `{}`", test_name),
                        WarnFlags::empty(),
                    )?;
                    Ok(false)
                } else {
                    Ok(true)
                }
            })),
        )))
    }
}

impl Foundry {
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
