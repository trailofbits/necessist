use crate::{LightContext, ToConsoleString};
use ansi_term::{
    Color::{Green, Yellow},
    Style,
};
use anyhow::{bail, Result};
use heck::ToKebabCase;
use lazy_static::lazy_static;
use std::{collections::BTreeSet, sync::Mutex};

#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
#[remain::sorted]
pub enum Warning {
    All,
    ConfigFilesExperimental,
    DryRunFailed,
    FilesChanged,
    IgnoredFunctionsUnsupported,
    IgnoredMacrosUnsupported,
    ModulePathUnknown,
    RunTestFailed,
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_kebab_case())
    }
}

pub(crate) fn source_warn(
    context: &LightContext,
    warning: Warning,
    source: &dyn ToConsoleString,
    msg: &str,
) -> Result<()> {
    warn_internal(context, warning, Some(source), msg, false)
}

pub(crate) fn warn(context: &LightContext, warning: Warning, msg: &str) -> Result<()> {
    warn_internal(context, warning, None, msg, false)
}

pub(crate) fn warn_once(context: &LightContext, warning: Warning, msg: &str) -> Result<()> {
    warn_internal(context, warning, None, msg, true)
}

lazy_static! {
    static ref WARNINGS_SHOWN: Mutex<BTreeSet<Warning>> = Mutex::new(BTreeSet::new());
    static ref ALLOW_MSG_SHOWN: Mutex<BTreeSet<Warning>> = Mutex::new(BTreeSet::new());
}

fn warn_internal(
    context: &LightContext,
    warning: Warning,
    source: Option<&dyn ToConsoleString>,
    msg: &str,
    once: bool,
) -> Result<()> {
    assert_ne!(warning, Warning::All);

    if context.opts.deny.contains(&Warning::All) || context.opts.deny.contains(&warning) {
        bail!(msg.to_owned());
    }

    if context.opts.quiet
        || context.opts.allow.contains(&Warning::All)
        || context.opts.allow.contains(&warning)
    {
        return Ok(());
    }

    if once {
        #[allow(clippy::unwrap_used)]
        let mut warnings_shown = WARNINGS_SHOWN.lock().unwrap();
        if warnings_shown.contains(&warning) {
            return Ok(());
        }
        warnings_shown.insert(warning);
    }

    let allow_msg = {
        #[allow(clippy::unwrap_used)]
        let mut allow_msg_shown = ALLOW_MSG_SHOWN.lock().unwrap();
        if allow_msg_shown.contains(&warning) {
            String::new()
        } else {
            allow_msg_shown.insert(warning);
            format!(
                "
Silence this warning with: --allow {}",
                warning
            )
        }
    };

    (context.println)(&format!(
        "{}{}: {}{}",
        source.map_or(String::new(), |source| format!(
            "{}: ",
            source.to_console_string()
        )),
        if atty::is(atty::Stream::Stdout) {
            Yellow.bold()
        } else {
            Style::default()
        }
        .paint("Warning"),
        msg,
        allow_msg
    ));

    Ok(())
}

pub(crate) fn note(context: &LightContext, msg: &str) {
    if context.opts.quiet {
        return;
    }

    (context.println)(&format!(
        "{}: {}",
        if atty::is(atty::Stream::Stdout) {
            Green.bold()
        } else {
            Style::default()
        }
        .paint("Note"),
        msg
    ));
}
