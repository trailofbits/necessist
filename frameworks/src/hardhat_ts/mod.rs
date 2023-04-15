use super::{ts_utils, ParseLow, RunHigh, WalkDirResult};
use anyhow::{anyhow, Result};
use assert_cmd::output::OutputError;
use lazy_static::lazy_static;
use log::debug;
use necessist_core::{
    framework::Postprocess, source_warn, warn, Config, LightContext, Span, WarnFlags, Warning,
};
use regex::Regex;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
};
use subprocess::{Exec, NullFile};
use swc_core::{
    common::SourceMap,
    ecma::{
        ast::{EsVersion, Module},
        parser::{lexer::Lexer, Parser, StringInput, Syntax, TsConfig},
    },
};

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

pub struct HardhatTs {
    source_map: Rc<SourceMap>,
    span_it_message_map: BTreeMap<Span, String>,
    test_file_it_message_state_map: RefCell<BTreeMap<PathBuf, BTreeMap<String, ItMessageState>>>,
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
            source_map: Rc::default(),
            span_it_message_map: BTreeMap::new(),
            test_file_it_message_state_map: RefCell::new(BTreeMap::new()),
        }
    }
}

lazy_static! {
    static ref LINE_WITH_TIME_RE: Regex = {
        // smoelius: The initial `.` is the check mark.
        #[allow(clippy::unwrap_used)]
        Regex::new(r"^\s*. (.*) \(.*\)$").unwrap()
    };
    static ref LINE_WITHOUT_TIME_RE: Regex = {
        #[allow(clippy::unwrap_used)]
        Regex::new(r"^\s*. (.*)$").unwrap()
    };
}

impl ParseLow for HardhatTs {
    type File = Module;

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

    fn walk_dir(root: &Path) -> Box<dyn Iterator<Item = WalkDirResult>> {
        Box::new(
            walkdir::WalkDir::new(root.join("test"))
                .into_iter()
                .filter_entry(|entry| {
                    let path = entry.path();
                    !path.is_file() || path.extension() == Some(OsStr::new("ts"))
                }),
        )
    }

    fn parse_file(&self, test_file: &Path) -> Result<Self::File> {
        let source_file = self.source_map.load_file(test_file)?;
        let lexer = Lexer::new(
            Syntax::Typescript(TsConfig::default()),
            EsVersion::default(),
            StringInput::from(&*source_file),
            None,
        );
        let mut parser = Parser::new_from(lexer);
        parser
            .parse_typescript_module()
            .map_err(|error| anyhow!(format!("{error:?}")))
    }

    fn visit(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_file: &Path,
        file: &Self::File,
    ) -> Result<Vec<Span>> {
        Ok(visit(
            config,
            self,
            self.source_map.clone(),
            context.root.clone(),
            test_file,
            file,
        ))
    }
}

impl RunHigh for HardhatTs {
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()> {
        ts_utils::install_node_modules(context)?;

        compile(context)?;

        let mut command = Command::new("npx");
        command.current_dir(context.root.as_path());
        command.args(["hardhat", "test", &test_file.to_string_lossy()]);
        command.args(&context.opts.args);

        debug!("{:?}", command);

        let output = command.output()?;
        if !output.status.success() {
            return Err(OutputError::new(output).into());
        }

        let mut test_file_it_message_state_map = self.test_file_it_message_state_map.borrow_mut();
        let it_message_state_map = test_file_it_message_state_map
            .entry(test_file.to_path_buf())
            .or_insert_with(Default::default);

        let stdout = std::str::from_utf8(&output.stdout)?;
        for line in stdout.lines() {
            if let Some(captures) = LINE_WITH_TIME_RE
                .captures(line)
                .or_else(|| LINE_WITHOUT_TIME_RE.captures(line))
            {
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
        if let Err(error) = compile(context) {
            debug!("{}", error);
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
                    &format!("`it` message {it_message:?} was not found during dry run"),
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
        exec = exec.cwd(context.root.as_path());
        exec = exec.args(&["hardhat", "test", &span.source_file.to_string_lossy()]);
        exec = exec.args(&context.opts.args);
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

fn compile(context: &LightContext) -> Result<()> {
    let mut command = Command::new("npx");
    command.current_dir(context.root.as_path());
    command.args(["hardhat", "compile"]);
    command.args(&context.opts.args);

    debug!("{:?}", command);

    let output = command.output()?;
    if !output.status.success() {
        return Err(OutputError::new(output).into());
    };
    Ok(())
}
