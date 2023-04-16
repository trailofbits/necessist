use super::{Low, ProcessLines};
use anyhow::{Context, Result};
use cargo_metadata::Package;
use necessist_core::{util, warn, Config, LightContext, Span, WarnFlags, Warning};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::read_to_string,
    path::{Path, PathBuf},
    process::Command,
};
use syn::parse_file;
use walkdir::WalkDir;

mod parsing;
use parsing::{cached_test_file_fs_module_path, cached_test_file_package, Parsing};

mod try_insert;
use try_insert::TryInsert;

mod visitor;
use visitor::visit;

#[derive(Debug)]
pub struct Rust {
    test_file_flags_cache: BTreeMap<PathBuf, Vec<String>>,
    span_test_path_map: BTreeMap<Span, Vec<String>>,
}

impl Rust {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context
            .root
            .join("Cargo.toml")
            .try_exists()
            .map_err(Into::into)
    }

    pub fn new() -> Self {
        Self {
            test_file_flags_cache: BTreeMap::new(),
            span_test_path_map: BTreeMap::new(),
        }
    }
}

impl Low for Rust {
    #[allow(clippy::similar_names)]
    #[cfg_attr(
        dylint_lib = "non_local_effect_before_error_return",
        allow(non_local_effect_before_error_return)
    )]
    #[cfg_attr(dylint_lib = "overscoped_allow", allow(overscoped_allow))]
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
            assert!(test_file.starts_with(context.root.as_path()));
            let content = read_to_string(test_file)?;
            #[allow(clippy::unwrap_used)]
            let file = parse_file(&content).with_context(|| {
                format!(
                    "Failed to parse {:?}",
                    util::strip_prefix(test_file, context.root).unwrap()
                )
            })?;
            let spans_visited = visit(context, config, self, &mut parsing, test_file, &file)?;
            spans.extend(spans_visited);
            Ok(())
        };

        if test_files.is_empty() {
            for entry in WalkDir::new(context.root.as_path())
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

    fn command_to_run_test_file(&self, context: &LightContext, test_file: &Path) -> Command {
        self.test_command(context, test_file)
    }

    fn command_to_build_test(&self, context: &LightContext, span: &Span) -> Command {
        let mut command = self.test_command(context, &span.source_file);
        command.arg("--no-run");
        command
    }

    fn command_to_run_test(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> (Command, Vec<String>, Option<(ProcessLines, String)>) {
        #[allow(clippy::expect_used)]
        let test_path = self
            .span_test_path_map
            .get(span)
            .expect("Test path is not set");
        let test = test_path.join("::");

        (
            self.test_command(context, &span.source_file),
            vec!["--".to_owned(), "--exact".to_owned(), test.clone()],
            Some(((false, Box::new(|line| line == "running 1 test")), test)),
        )
    }
}

fn check_config(context: &LightContext, config: &Config) -> Result<()> {
    if !config.ignored_functions.is_empty() {
        warn(
            context,
            Warning::IgnoredFunctionsUnsupported,
            "The rust framework does not currently support the `ignored_functions` configuration",
            WarnFlags::ONCE,
        )?;
    }

    Ok(())
}

impl Rust {
    fn test_command(&self, _context: &LightContext, test_file: &Path) -> Command {
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

    #[cfg_attr(
        dylint_lib = "non_local_effect_before_error_return",
        allow(non_local_effect_before_error_return)
    )]
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
