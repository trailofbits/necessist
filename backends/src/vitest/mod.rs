use super::{OutputAccessors, OutputStrippedOfAnsiScapes, ParseAdapter, ParseHigh, RunHigh, ts};
use anyhow::Result;
use log::debug;
use necessist_core::{
    __Rewriter as Rewriter, LightContext, SourceFile, Span,
    framework::{Interface, Postprocess, SourceFileSpanTestMap},
};
use std::{path::Path, process::Command};
use subprocess::Exec;

pub struct Vitest {
    mocha_adapter: ParseAdapter<Box<dyn ts::MochaLike>>,
}

impl Vitest {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context
            .root
            .join("vitest.config.ts")
            .try_exists()
            .map_err(Into::into)
    }

    pub fn new() -> Self {
        Self {
            mocha_adapter: ParseAdapter(Box::new(ts::Vitest::new(".", None))),
        }
    }
}

impl Interface for Vitest {}

impl ParseHigh for Vitest {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &necessist_core::config::Toml,
        source_files: &[&Path],
    ) -> Result<(usize, SourceFileSpanTestMap)> {
        self.mocha_adapter.parse(context, config, source_files)
    }
}

impl RunHigh for Vitest {
    fn dry_run(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        ts::utils::install_node_modules(context)?;

        let command = command_to_run_test(context, source_file);

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

    // smoelius: `build_source_file` helps Necessist identify bugs in its instrumentation. For
    // Vitest, I have no better idea for "building" a source file than to just run all of the file's
    // tests.
    fn build_source_file(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        let mut command = command_to_run_test(context, source_file);

        debug!("{command:?}");

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
    ) -> Result<Result<(Exec, Option<Box<Postprocess>>)>> {
        let command = command_to_run_test(context, &span.source_file);

        self.mocha_adapter
            .0
            .exec(context, test_name, span, &command)
    }
}

fn command_to_run_test(context: &LightContext, source_file: &Path) -> Command {
    let mut command = ts::utils::script("npx");
    command.current_dir(context.root.as_path());
    command.args(["vitest", "run", &source_file.to_string_lossy()]);
    command.args(ts::VITEST_COMMAND_SUFFIX.split_ascii_whitespace());
    command.args(&context.opts.args);

    command
}
