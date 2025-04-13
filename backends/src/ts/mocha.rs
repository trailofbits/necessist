use super::{Inner, MochaLike};
use anyhow::Result;
use necessist_core::{LightContext, Span, framework::Postprocess};
use regex::Regex;
use std::{path::Path, process::Command, sync::LazyLock};
use subprocess::Exec;

pub struct Mocha(Inner);

impl Mocha {
    pub fn new(subdir: impl AsRef<Path>) -> Self {
        Self(Inner::new(subdir, None, Box::new(it_message_extractor)))
    }
}

static LINE_WITH_TIME_RE: LazyLock<Regex> = LazyLock::new(|| {
    // smoelius: The initial `.` is the check mark.
    #[allow(clippy::unwrap_used)]
    Regex::new(r"^\s*. (.*) \([0-9]+ms\)$").unwrap()
});

static LINE_WITHOUT_TIME_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::unwrap_used)]
    Regex::new(r"^\s*. (.*)$").unwrap()
});

pub fn it_message_extractor(bytes: &[u8]) -> Result<Vec<String>> {
    let stdout = std::str::from_utf8(bytes)?;
    let mut it_messages = Vec::new();
    for line in stdout.lines() {
        if let Some(captures) = LINE_WITH_TIME_RE
            .captures(line)
            .or_else(|| LINE_WITHOUT_TIME_RE.captures(line))
        {
            assert_eq!(2, captures.len());
            it_messages.push(captures[1].to_string());
        }
    }
    Ok(it_messages)
}

impl MochaLike for Mocha {
    fn as_inner(&self) -> &Inner {
        &self.0
    }

    fn as_inner_mut(&mut self) -> &mut Inner {
        &mut self.0
    }

    fn dry_run(&self, context: &LightContext, source_file: &Path, command: Command) -> Result<()> {
        self.0.dry_run(context, source_file, command)
    }

    fn statement_prefix_and_suffix(&self, span: &Span) -> Result<(String, String)> {
        self.0.statement_prefix_and_suffix(span)
    }

    fn exec(
        &self,
        context: &LightContext,
        test_name: &str,
        span: &Span,
        command: &Command,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>> {
        self.0.exec(context, test_name, span, command)
    }
}
