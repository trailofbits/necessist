use super::{ts, OutputAccessors, OutputStrippedOfAnsiScapes, ParseAdapter, ParseHigh, RunHigh};
use anyhow::Result;
use log::debug;
use necessist_core::{
    framework::{Interface, Postprocess, TestFileTestSpanMap},
    LightContext, Span,
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
        test_files: &[&Path],
    ) -> Result<TestFileTestSpanMap> {
        self.mocha_adapter.parse(context, config, test_files)
    }
}

impl RunHigh for HardhatTs {
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()> {
        ts::utils::install_node_modules(context)?;

        compile(context)?;

        let mut command = ts::utils::script("npx");
        command.current_dir(context.root.as_path());
        command.args(["hardhat", "test", &test_file.to_string_lossy()]);
        command.args(&context.opts.args);

        self.mocha_adapter.0.dry_run(context, test_file, command)
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
