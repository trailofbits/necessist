use super::{Interface, Postprocess};
use crate::{util, warn_once, Config, LightContext, Span, WarnKey};
use anyhow::{anyhow, ensure, Context, Result};
use log::debug;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
};
use subprocess::{Exec, NullFile};
use swc_core::{
    common::SourceMap,
    ecma::{
        ast::EsVersion,
        parser::{lexer::Lexer, Parser, StringInput, Syntax, TsConfig},
    },
};
use walkdir::WalkDir;

mod visitor;
use visitor::visit;

#[derive(Debug, Default)]
pub(super) struct HardhatTs {
    root: Option<Rc<PathBuf>>,
}

impl Interface for HardhatTs {
    fn applicable(&self, context: &LightContext) -> Result<bool> {
        Ok(context.root.join("hardhat.config.ts").exists())
    }

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
        if self.root.is_none() {
            self.root = Some(Rc::new(context.root.to_path_buf()));
        }

        check_config(context, config);

        let mut spans = Vec::new();

        let mut visit_test_file = |test_file: &Path| -> Result<()> {
            assert!(test_file.is_absolute());
            assert!(test_file.starts_with(context.root));
            let source_map: Rc<SourceMap> = Rc::default();
            let source_file = source_map.load_file(test_file)?;
            let lexer = Lexer::new(
                Syntax::Typescript(TsConfig::default()),
                EsVersion::default(),
                StringInput::from(&*source_file),
                None,
            );
            let mut parser = Parser::new_from(lexer);
            #[allow(clippy::unwrap_used)]
            let module = parser
                .parse_typescript_module()
                .map_err(|error| anyhow!(format!("{:?}", error)))
                .with_context(|| {
                    format!(
                        "Failed to parse {:?}",
                        util::strip_prefix(test_file, context.root).unwrap()
                    )
                })?;
            #[allow(clippy::expect_used)]
            let visited_spans = visit(
                config,
                source_map,
                self.root.as_ref().expect("`root` is unset").clone(),
                test_file,
                &module,
            );
            spans.extend(visited_spans);
            Ok(())
        };

        if test_files.is_empty() {
            for entry in WalkDir::new(context.root.join("test")) {
                let entry = entry?;
                let path = entry.path();

                if path.extension() != Some(OsStr::new("ts")) {
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
        compile(context)?;

        let mut command = Command::new("npx");
        command.current_dir(context.root);
        command.args(["hardhat", "test", &test_file.to_string_lossy()]);

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
        if compile(context).is_err() {
            return Ok(None);
        }

        let mut exec = Exec::cmd("npx");
        exec = exec.cwd(context.root);
        exec = exec.args(&["hardhat", "test", &span.source_file.to_string_lossy()]);
        exec = exec.stdout(NullFile);
        exec = exec.stderr(NullFile);

        debug!("{:?}", exec);

        Ok(Some((exec, None)))
    }
}

fn check_config(context: &LightContext, config: &Config) {
    if !config.ignored_macros.is_empty() {
        warn_once(
            context,
            "The hardhat-ts framework does not support the `ignored_macros` configuration",
            WarnKey::HardhatTsIgnoredMacros,
        );
    }
}

fn compile(context: &LightContext) -> Result<()> {
    let mut command = Command::new("npx");
    command.current_dir(context.root);
    command.args(["hardhat", "compile"]);
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());

    debug!("{:?}", command);

    let output = command.output()?;
    ensure!(output.status.success(), "{:#?}", output);
    Ok(())
}
