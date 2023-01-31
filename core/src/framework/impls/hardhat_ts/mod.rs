use super::super::{Interface, Postprocess};
use crate::{source_warn, util, warn, Config, LightContext, Span, WarnFlags, Warning};
use anyhow::{anyhow, ensure, Context, Result};
use lazy_static::lazy_static;
use log::debug;
use regex::Regex;
use std::{
    cell::RefCell,
    collections::BTreeMap,
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

#[derive(Debug, Eq, PartialEq)]
enum ItMessageState {
    NotFound,
    Found,
    WarningEmitted,
}

impl Default for ItMessageState {
    fn default() -> Self {
        Self::NotFound
    }
}

#[derive(Debug)]
pub struct HardhatTs {
    root: Rc<PathBuf>,
    span_it_message_map: BTreeMap<Span, String>,
    test_file_it_message_state_map: RefCell<BTreeMap<PathBuf, BTreeMap<String, ItMessageState>>>,
}

impl HardhatTs {
    pub fn applicable(context: &LightContext) -> Result<Option<Self>> {
        if context.root.join("hardhat.config.ts").try_exists()? {
            Ok(Some(Self::new(context)))
        } else {
            Ok(None)
        }
    }

    fn new(context: &LightContext) -> Self {
        Self {
            root: Rc::new(context.root.to_path_buf()),
            span_it_message_map: BTreeMap::new(),
            test_file_it_message_state_map: RefCell::new(BTreeMap::new()),
        }
    }
}

lazy_static! {
    static ref RE: Regex = {
        #[allow(clippy::unwrap_used)]
        Regex::new(r"^\s*✔ (.*) \(.*\)$").unwrap()
    };
}

impl Interface for HardhatTs {
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
                .map_err(|error| anyhow!(format!("{error:?}")))
                .with_context(|| {
                    format!(
                        "Failed to parse {:?}",
                        util::strip_prefix(test_file, context.root).unwrap()
                    )
                })?;
            let spans_visited = visit(
                config,
                self,
                source_map,
                self.root.clone(),
                test_file,
                &module,
            );
            spans.extend(spans_visited);
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

        let mut test_file_it_message_state_map = self.test_file_it_message_state_map.borrow_mut();
        let it_message_state_map = test_file_it_message_state_map
            .entry(test_file.to_path_buf())
            .or_insert_with(Default::default);

        let stdout = std::str::from_utf8(&output.stdout)?;
        for line in stdout.lines() {
            if let Some(captures) = RE.captures(line) {
                assert!(captures.len() == 2);
                it_message_state_map.insert(captures[1].to_string(), ItMessageState::Found);
            }
        }

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

        #[allow(clippy::expect_used)]
        let it_message = self
            .span_it_message_map
            .get(span)
            .expect("`it` message is not set");

        let mut test_file_it_message_state_map = self.test_file_it_message_state_map.borrow_mut();
        #[allow(clippy::expect_used)]
        let it_message_state_map = test_file_it_message_state_map
            .get_mut(span.source_file.as_ref())
            .expect("Source file is not in map");

        let state = it_message_state_map
            .entry(it_message.clone())
            .or_insert_with(Default::default);
        if *state != ItMessageState::Found {
            if *state == ItMessageState::NotFound {
                source_warn(
                    context,
                    Warning::ItMessageNotFound,
                    span,
                    &format!("`it` messages {it_message:?} was not found during dry run"),
                    WarnFlags::empty(),
                )?;
                *state = ItMessageState::WarningEmitted;
            }
            // smoelius: Returning `None` here causes Necessist to associate `Outcome::Nonbuildable`
            // with this span. This is not ideal, but there is no ideal choice for this situation
            // currently.
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

impl HardhatTs {
    fn set_span_it_message(&mut self, span: &Span, it_message: String) {
        self.span_it_message_map.insert(span.clone(), it_message);
    }
}

fn check_config(context: &LightContext, config: &Config) -> Result<()> {
    if !config.ignored_macros.is_empty() {
        warn(
            context,
            Warning::IgnoredMacrosUnsupported,
            "The hardhat-ts framework does not support the `ignored_macros` configuration",
            WarnFlags::ONCE,
        )?;
    }

    Ok(())
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
