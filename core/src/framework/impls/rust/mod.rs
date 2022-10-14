use super::super::{Interface, Postprocess};
use crate::{source_warn, util, warn_once, Config, LightContext, Span, TryInsert, Warning};
use anyhow::{anyhow, ensure, Context, Result};
use cargo_metadata::Package;
use log::debug;
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::read_to_string,
    io::BufRead,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};
use subprocess::{Exec, NullFile, Redirection};
use syn::parse_file;
use walkdir::WalkDir;

mod parsing;
use parsing::{cached_test_file_fs_module_path, cached_test_file_package, Parsing};

mod visitor;
use visitor::visit;

const BUG_MSG: &str = "

This may indicate a bug in Necessist. Consider opening an issue at: \
https://github.com/trailofbits/necessist/issues
";

static BUG_MSG_SHOWN: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
pub struct Rust {
    root: Rc<PathBuf>,
    test_file_flags_cache: BTreeMap<PathBuf, Vec<String>>,
    span_test_path_map: BTreeMap<Span, Vec<String>>,
}

impl Rust {
    pub fn applicable(context: &LightContext) -> Result<Option<Self>> {
        if context.root.join("Cargo.toml").try_exists()? {
            Ok(Some(Self::new(context)))
        } else {
            Ok(None)
        }
    }

    fn new(context: &LightContext) -> Self {
        Self {
            root: Rc::new(context.root.to_path_buf()),
            test_file_flags_cache: BTreeMap::new(),
            span_test_path_map: BTreeMap::new(),
        }
    }
}

impl Interface for Rust {
    #[allow(clippy::similar_names)]
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

        let mut parsing = Parsing::default();
        let mut spans = Vec::new();

        #[cfg_attr(
            dylint_lib = "non_local_effect_before_error_return",
            allow(non_local_effect_before_error_return)
        )]
        let mut visit_test_file = |test_file: &Path| -> Result<()> {
            assert!(test_file.is_absolute());
            assert!(test_file.starts_with(context.root));
            let content = read_to_string(test_file)?;
            #[allow(clippy::unwrap_used)]
            let file = parse_file(&content).with_context(|| {
                format!(
                    "Failed to parse {:?}",
                    util::strip_prefix(test_file, context.root).unwrap()
                )
            })?;
            let spans_visited = visit(
                context,
                config,
                self,
                &mut parsing,
                self.root.clone(),
                test_file,
                &file,
            )?;
            spans.extend(spans_visited);
            Ok(())
        };

        if test_files.is_empty() {
            for entry in WalkDir::new(context.root)
                .into_iter()
                .filter_entry(|entry| entry.path().file_name() != Some(OsStr::new("target")))
            {
                let entry = entry?;
                let path = entry.path();

                if path.extension() != Some(OsStr::new("rs")) {
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
        let mut command = self.build_test_command(context, test_file);

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
        let test_path = self
            .span_test_path_map
            .get(span)
            .expect("Test path is not set");
        let test = test_path.join("::");

        {
            let mut command = self.build_test_command(context, &span.source_file);
            command.arg("--no-run");
            command.stdout(Stdio::null());
            command.stderr(Stdio::null());

            debug!("{:?}", command);

            let status = command.status()?;
            if !status.success() {
                return Ok(None);
            }
        }

        let mut exec = self.build_test_exec(context, &span.source_file);
        exec = exec.args(&["--", "--exact", &test]);
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
                let running_1_test = reader.lines().try_fold(false, |prev, line| {
                    let line = line?;
                    Ok::<_, std::io::Error>(prev || line == "running 1 test")
                })?;
                if running_1_test {
                    return Ok(true);
                }
                let mut msg = format!("Failed to run test `{}`", test);
                if !BUG_MSG_SHOWN.load(Ordering::SeqCst) {
                    BUG_MSG_SHOWN.store(true, Ordering::SeqCst);
                    msg += BUG_MSG;
                }
                source_warn(context, Warning::RunTestFailed, &span, &msg)?;
                Ok(false)
            })),
        )))
    }
}

fn check_config(context: &LightContext, config: &Config) -> Result<()> {
    if !config.ignored_functions.is_empty() {
        warn_once(
            context,
            Warning::IgnoredFunctionsUnsupported,
            "The rust framework does not currently support the `ignored_functions` configuration",
        )?;
    }

    Ok(())
}

impl Rust {
    fn build_test_command(&self, _context: &LightContext, test_file: &Path) -> Command {
        #[allow(clippy::expect_used)]
        let flags = self
            .test_file_flags_cache
            .get(test_file)
            .expect("Flags are not cached");
        let mut command = Command::new("cargo");
        command.arg("test");
        command.args(flags);
        command
    }

    fn build_test_exec(&self, _context: &LightContext, test_file: &Path) -> Exec {
        #[allow(clippy::expect_used)]
        let flags = self
            .test_file_flags_cache
            .get(test_file)
            .expect("Flags are not cached");
        let mut exec = Exec::cmd("cargo");
        exec = exec.arg("test");
        exec = exec.args(flags);
        exec
    }

    fn cached_test_file_flags(
        &mut self,
        test_file_package_map: &mut BTreeMap<PathBuf, Package>,
        test_file: &Path,
    ) -> Result<&Vec<String>> {
        self.test_file_flags_cache
            .entry(test_file.to_path_buf())
            .or_try_insert_with(|| {
                let package = cached_test_file_package(test_file_package_map, test_file)?;

                let mut flags = vec![
                    "--manifest-path".to_owned(),
                    package.manifest_path.as_str().to_owned(),
                ];

                if let Some(name) = test_file_test(package, test_file) {
                    flags.extend(["--test".to_owned(), name.clone()]);
                } else {
                    // smoelius: Failed to find a test target with this file name. Assume it is a unit test.
                    for kind in package.targets.iter().flat_map(|target| &target.kind) {
                        match kind.as_ref() {
                            "bin" => flags.push("--bins".to_owned()),
                            "lib" => flags.push("--lib".to_owned()),
                            _ => {}
                        }
                    }
                }

                Ok(flags)
            })
            .map(|value| value as &_)
    }

    fn set_span_test_path(&mut self, span: &Span, test_path: Vec<String>) {
        self.span_test_path_map.insert(span.clone(), test_path);
    }
}

fn test_file_test<'a>(package: &'a Package, test_file: &Path) -> Option<&'a String> {
    if let &[name] = package
        .targets
        .iter()
        .filter_map(|target| {
            if target.kind == ["test"] && target.src_path == test_file {
                Some(&target.name)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .as_slice()
    {
        Some(name)
    } else {
        None
    }
}
