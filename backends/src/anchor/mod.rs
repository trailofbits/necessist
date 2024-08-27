use super::{ts, OutputAccessors, OutputStrippedOfAnsiScapes, ParseAdapter, ParseHigh, RunHigh};
use anyhow::{anyhow, Context, Result};
use log::debug;
use necessist_core::{
    framework::{Interface, Postprocess, SourceFileSpanTestMap},
    LightContext, SourceFile, Span, __Backup as Backup, __Rewriter as Rewriter,
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    fs::{read_to_string, write},
    path::{Path, PathBuf},
    process::Command,
};
use subprocess::Exec;
use toml_edit::{DocumentMut, Value};

pub struct Anchor {
    mocha_adapter: ParseAdapter<ts::mocha::Mocha>,
    anchor_toml: PathBuf,
    document: DocumentMut,
    prefix: String,
    suffix: String,
}

impl Anchor {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context
            .root
            .join("Anchor.toml")
            .try_exists()
            .map_err(Into::into)
    }

    pub fn new(context: &LightContext) -> Result<Self> {
        let anchor_toml = context.root.join("Anchor.toml");
        let contents = read_to_string(&anchor_toml)
            .with_context(|| format!(r#"Failed to read "{}""#, anchor_toml.display()))?;
        let mut document = contents.parse::<DocumentMut>()?;
        let (prefix, suffix) = edit_test_script(&mut document, parse_test_value)?;
        Ok(Self {
            mocha_adapter: ParseAdapter(ts::mocha::Mocha::new("tests")),
            anchor_toml,
            document,
            prefix,
            suffix,
        })
    }
}

impl Interface for Anchor {}

impl ParseHigh for Anchor {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &necessist_core::config::Toml,
        source_files: &[&Path],
    ) -> Result<SourceFileSpanTestMap> {
        self.mocha_adapter.parse(context, config, source_files)
    }
}

impl RunHigh for Anchor {
    fn dry_run(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        ts::utils::install_node_modules(context)?;

        self.check(context, source_file)?;

        let _backup: Backup = self.patch_anchor_toml(source_file, false)?;

        let command = command_to_run_test(context);

        self.mocha_adapter.0.dry_run(context, source_file, command)
    }

    fn instrument_source_file(
        &self,
        _context: &LightContext,
        _rewriter: &mut Rewriter,
        _source_file: &SourceFile,
        _n_instrumentable_statements: usize,
    ) -> Result<()> {
        Ok(())
    }

    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        self.mocha_adapter.0.statement_prefix_and_suffix(span)
    }

    fn build_source_file(&self, context: &LightContext, _source_file: &Path) -> Result<()> {
        let mut command = command_to_build_source_file(context);

        debug!("{:?}", command);

        let output = command.output_stripped_of_ansi_escapes()?;
        if !output.status().success() {
            return Err(output.into());
        }
        Ok(())
    }

    fn exec(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        if let Err(error) = self.check(context, &span.source_file) {
            debug!("{}", error);
            return Ok(None);
        }

        let backup = self.patch_anchor_toml(&span.source_file, false)?;

        let command = command_to_run_test(context);

        let exec_and_postprocess = self
            .mocha_adapter
            .0
            .exec(context, test_name, span, &command)?;

        Ok(exec_and_postprocess.map(|(exec, postprocess)| {
            let postprocess: Box<Postprocess> = Box::new(move |context, popen| {
                // smoelius: Ensure `backup` hasn't been dropped yet;
                let _: &Backup = &backup;
                if let Some(postprocess) = &postprocess {
                    postprocess(context, popen)
                } else {
                    Ok(true)
                }
            });
            (exec, Some(postprocess))
        }))
    }
}

impl Anchor {
    fn check(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        let _backup: Backup = self.patch_anchor_toml(source_file, true)?;

        let mut command = command_to_run_test(context);

        debug!("{:?}", command);

        let output = command.output_stripped_of_ansi_escapes()?;
        if !output.status().success() {
            return Err(output.into());
        };
        Ok(())
    }

    fn patch_anchor_toml(&self, source_file: &Path, check: bool) -> Result<Backup> {
        let backup = Backup::new(&self.anchor_toml)?;

        let mut document = self.document.clone();

        edit_test_script(&mut document, |test| {
            *test = Value::from(format!(
                "{}{}{}{}",
                self.prefix,
                source_file.to_string_lossy(),
                self.suffix,
                if check { " --dry-run" } else { "" }
            ));
            Ok(())
        })
        .expect("Document is not parsable");

        write(&self.anchor_toml, document.to_string())?;

        Ok(backup)
    }
}

static TEST_RE: Lazy<Regex> = Lazy::new(|| {
    // smoelius: If the space in the first capture group `(.* )` is replaced with `\b`, then the
    // capture group captures too much.
    #[allow(clippy::unwrap_used)]
    Regex::new(r"^(.* )[^ ]*\.ts\b(.*)$").unwrap()
});

#[cfg_attr(dylint_lib = "general", allow(non_local_effect_before_error_return))]
fn edit_test_script<T>(
    document: &mut DocumentMut,
    f: impl FnOnce(&mut Value) -> Result<T>,
) -> Result<T> {
    let table = document.as_table_mut();
    let script = table
        .get_mut("scripts")
        .ok_or_else(|| anyhow!("Failed to find `scripts` key"))?;
    let scripts_table = script
        .as_table_mut()
        .ok_or_else(|| anyhow!("`scripts` is not a table"))?;
    let test = scripts_table
        .get_mut("test")
        .ok_or_else(|| anyhow!("Failed to find `test` key"))?;
    let test_value = test
        .as_value_mut()
        .ok_or_else(|| anyhow!("`test` is not a value"))?;
    f(test_value)
}

fn parse_test_value(test_value: &mut Value) -> Result<(String, String)> {
    let test_str = test_value
        .as_str()
        .ok_or_else(|| anyhow!("`test` is not a string"))?;
    let captures = TEST_RE
        .captures(test_str)
        .ok_or_else(|| anyhow!("Failed to parse `test` string: {test_str:?}"))?;
    assert_eq!(3, captures.len());
    Ok((captures[1].to_string(), captures[2].to_string()))
}

fn command_to_build_source_file(context: &LightContext) -> Command {
    let mut command = Command::new("anchor");
    command.arg("build");
    command.args(&context.opts.args);
    command.current_dir(context.root.as_path());

    command
}

fn command_to_run_test(context: &LightContext) -> Command {
    let mut command = Command::new("anchor");
    command.arg("test");
    command.args(&context.opts.args);
    command.current_dir(context.root.as_path());

    command
}
