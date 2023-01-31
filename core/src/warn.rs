use crate::{LightContext, ToConsoleString};
use ansi_term::{
    Color::{Green, Yellow},
    Style,
};
use anyhow::{bail, Result};
use bitflags::bitflags;
use heck::ToKebabCase;
use lazy_static::lazy_static;
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[non_exhaustive]
#[remain::sorted]
pub enum Warning {
    All,
    ConfigFilesExperimental,
    DryRunFailed,
    FilesChanged,
    IgnoredFunctionsUnsupported,
    IgnoredMacrosUnsupported,
    ItMessageNotFound,
    ModulePathUnknown,
    RunTestFailed,
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_kebab_case())
    }
}

bitflags! {
    pub struct Flags: u8 {
        const ONCE = 1 << 0;
    }
}

#[allow(clippy::module_name_repetitions)]
pub fn source_warn(
    context: &LightContext,
    warning: Warning,
    source: &dyn ToConsoleString,
    msg: &str,
    flags: Flags,
) -> Result<()> {
    warn_internal(context, warning, Some(source), msg, flags)
}

pub fn warn(context: &LightContext, warning: Warning, msg: &str, flags: Flags) -> Result<()> {
    warn_internal(context, warning, None, msg, flags)
}

const BUG_MSG: &str = "

This may indicate a bug in Necessist. Consider opening an issue at: \
https://github.com/trailofbits/necessist/issues
";

bitflags! {
    struct State: u8 {
        const ALLOW_MSG_EMITTED = 1 << 0;
        const BUG_MSG_EMITTED = 1 << 1;
        const WARNING_EMITTED = 1 << 2;
    }
}

lazy_static! {
    static ref WARNING_STATE_MAP: Mutex<BTreeMap<Warning, State>> = Mutex::new(BTreeMap::new());
}

#[cfg_attr(
    dylint_lib = "non_local_effect_before_error_return",
    allow(non_local_effect_before_error_return)
)]
fn warn_internal(
    context: &LightContext,
    warning: Warning,
    source: Option<&dyn ToConsoleString>,
    msg: &str,
    flags: Flags,
) -> Result<()> {
    assert_ne!(warning, Warning::All);

    #[allow(clippy::unwrap_used)]
    let mut warning_state_map = WARNING_STATE_MAP.lock().unwrap();

    let state = warning_state_map
        .entry(warning)
        .or_insert_with(State::empty);

    // smoelius: Append `BUG_MSG` to `msg` in case we have to `bail!`.
    let msg = msg.to_owned()
        + if may_be_bug(warning) && !state.contains(State::BUG_MSG_EMITTED) {
            state.insert(State::BUG_MSG_EMITTED);
            BUG_MSG
        } else {
            ""
        };

    if context.opts.deny.contains(&Warning::All) || context.opts.deny.contains(&warning) {
        bail!(msg);
    }

    if context.opts.quiet
        || context.opts.allow.contains(&Warning::All)
        || context.opts.allow.contains(&warning)
        || (flags.contains(Flags::ONCE) && state.contains(State::WARNING_EMITTED))
    {
        return Ok(());
    }

    let allow_msg = if state.contains(State::ALLOW_MSG_EMITTED) {
        String::new()
    } else {
        state.insert(State::ALLOW_MSG_EMITTED);
        format!(
            "
Silence this warning with: --allow {warning}"
        )
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

    state.insert(State::WARNING_EMITTED);

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

fn may_be_bug(warning: Warning) -> bool {
    match warning {
        Warning::All => unreachable!(),
        Warning::ConfigFilesExperimental
        | Warning::DryRunFailed
        | Warning::FilesChanged
        | Warning::IgnoredFunctionsUnsupported
        | Warning::IgnoredMacrosUnsupported
        | Warning::ItMessageNotFound => false,
        Warning::ModulePathUnknown | Warning::RunTestFailed => true,
    }
}
