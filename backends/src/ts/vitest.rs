use super::{Inner, MochaLike};
use anyhow::{Result, bail};
use necessist_core::{LightContext, Span, framework::Postprocess};
use serde_json::Value;
use std::{ffi::OsStr, path::Path, process::Command};
use subprocess::Exec;

// smoelius: Ideally, this constant would not be exposed. However, the Anchor backend uses it to
// patch the `test` script in the Anchor.toml file, and the Vitest backend appends it to the command
// passed to `MochaInner::exec`. So at present, I cannot see a better way.
pub const VITEST_COMMAND_SUFFIX: &str = " --bail=1 --reporter=json";

pub struct Vitest(Inner);

impl Vitest {
    #[allow(clippy::type_complexity)]
    pub fn new(
        subdir: impl AsRef<Path>,
        json_extractor: Option<&'static dyn Fn(&str) -> Result<&str>>,
    ) -> Self {
        let it_message_extractor = move |bytes: &'_ [u8]| {
            let stdout = std::str::from_utf8(bytes)?;
            let json = if let Some(json_extractor) = json_extractor {
                json_extractor(stdout)?
            } else {
                stdout
            };
            extract_it_messages(json)
        };

        Self(Inner::new(
            subdir,
            Some(&path_predicate),
            Box::new(it_message_extractor),
        ))
    }
}

fn path_predicate(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_none_or(|filename| !filename.ends_with(".test-d.ts"))
}

pub fn extract_it_messages(json: &str) -> Result<Vec<String>> {
    let value = json.parse::<Value>()?;
    let Some(test_results) = value
        .as_object()
        .and_then(|object| object.get("testResults"))
        .and_then(Value::as_array)
    else {
        bail!("Failed to find `testResults` in Vitest JSON output");
    };
    let assertion_results = test_results
        .iter()
        .filter_map(|value| {
            value
                .as_object()
                .and_then(|object| object.get("assertionResults"))
                .and_then(Value::as_array)
        })
        .flatten();
    let it_messages = assertion_results
        .filter_map(|value| {
            value
                .as_object()
                .and_then(|object| object.get("title"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect();
    Ok(it_messages)
}

impl MochaLike for Vitest {
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
