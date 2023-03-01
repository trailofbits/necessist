use anyhow::{anyhow, ensure, Context, Result};
use log::debug;
use necessist_core::{
    framework::{Interface, Postprocess},
    source_warn, util, warn, Config, LightContext, Span, WarnFlags, Warning,
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
    pub fn applicable(context: &LightContext) -> Result<Option<Self>> {
        if context.root.join("go.mod").try_exists()? {
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

impl Interface for Golang {
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

    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()> {
        let mut command = Self::build_test_command(context, test_file);
        command.args(&context.opts.args);

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
            .expect("Test name is not set");

        {
            let mut command = Self::build_test_command(context, &span.source_file);
            command.arg("-run=^$");
            command.args(&context.opts.args);
            command.stdout(Stdio::null());
            command.stderr(Stdio::null());

            debug!("{:?}", command);

            let status = command.status()?;
            if !status.success() {
                return Ok(None);
            }
        }

        let mut exec = Self::build_test_exec(context, &span.source_file);
        exec = exec.args(&[format!("-run=^{test_name}$").as_ref(), "-v"]);
        exec = exec.args(&context.opts.args);
        exec = exec.stdout(Redirection::Pipe);
        exec = exec.stderr(NullFile);

        let test_name = test_name.clone();
        let span = span.clone();

        Ok(Some((
            exec,
            Some(Box::new(move |context: &LightContext, popen| {
                let stdout = popen
                    .stdout
                    .as_ref()
                    .ok_or_else(|| anyhow!("Failed to get stdout"))?;
                let reader = std::io::BufReader::new(stdout);
                let run = reader.lines().try_fold(false, |prev, line| {
                    let line = line?;
                    Ok::<_, std::io::Error>(prev || line == format!("=== RUN   {test_name}"))
                })?;
                if run {
                    return Ok(true);
                }
                source_warn(
                    context,
                    Warning::RunTestFailed,
                    &span,
                    &format!("Failed to run test `{test_name}`"),
                    WarnFlags::empty(),
                )?;
                Ok(false)
            })),
        )))
    }
}

impl Golang {
    fn build_test_command(_context: &LightContext, test_file: &Path) -> Command {
        #[allow(clippy::expect_used)]
        let package_path =
            test_file_package_path(test_file).expect("Failed to get test file package path");
        let mut command = Command::new("go");
        command.arg("test");
        command.arg(package_path);
        command
    }

    fn build_test_exec(_context: &LightContext, test_file: &Path) -> Exec {
        #[allow(clippy::expect_used)]
        let package_path =
            test_file_package_path(test_file).expect("Failed to get test file package path");
        let mut exec = Exec::cmd("go");
        exec = exec.arg("test");
        exec = exec.arg(package_path);
        exec
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

fn test_file_package_path(test_file: &Path) -> Result<String> {
    let dir = test_file
        .parent()
        .ok_or_else(|| anyhow!("Failed to get parent"))?;

    assert!(dir.is_absolute());

    Ok(dir.to_string_lossy().to_string())
}
