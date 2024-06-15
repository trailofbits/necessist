use super::{ts, OutputAccessors, OutputStrippedOfAnsiScapes, ParseAdapter, ParseHigh, RunHigh};
use anyhow::Result;
use log::debug;
use necessist_core::{
    framework::{Interface, Postprocess, SourceFileTestSpanMap},
    LightContext, SourceFile, Span, __Rewriter as Rewriter,
};
use std::path::Path;
use subprocess::Exec;

pub struct HardhatTs {
    mocha_adapter: ParseAdapter<ts::mocha::Mocha>,
}

impl HardhatTs {
    pub fn applicable(context: &LightContext) -> Result<bool> {
        context
            .root
            .join("hardhat.config.ts")
            .try_exists()
            .map_err(Into::into)
    }

    pub fn new() -> Self {
        Self {
            mocha_adapter: ParseAdapter(ts::mocha::Mocha::new("test")),
        }
    }
}

impl Interface for HardhatTs {}

impl ParseHigh for HardhatTs {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &necessist_core::config::Toml,
        source_files: &[&Path],
    ) -> Result<SourceFileTestSpanMap> {
        self.mocha_adapter.parse(context, config, source_files)
    }
}

impl RunHigh for HardhatTs {
    fn dry_run(&self, context: &LightContext, source_file: &Path) -> Result<()> {
        ts::utils::install_node_modules(context)?;

        compile(context)?;

        let mut command = ts::utils::script("npx");
        command.current_dir(context.root.as_path());
        command.args(["hardhat", "test", &source_file.to_string_lossy()]);
        command.args(&context.opts.args);

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
        compile(context)
    }

    fn exec(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        if let Err(error) = compile(context) {
            debug!("{}", error);
            return Ok(None);
        }

        let mut command = ts::utils::script("npx");
        command.current_dir(context.root.as_path());
        command.args(["hardhat", "test", &span.source_file.to_string_lossy()]);
        command.args(&context.opts.args);

        self.mocha_adapter
            .0
            .exec(context, test_name, span, &command)
    }
}

fn compile(context: &LightContext) -> Result<()> {
    let mut command = ts::utils::script("npx");
    command.current_dir(context.root.as_path());
    command.args(["hardhat", "compile"]);
    command.args(&context.opts.args);

    debug!("{:?}", command);

    let output = command.output_stripped_of_ansi_escapes()?;
    if !output.status().success() {
        return Err(output.into());
    };
    Ok(())
}
