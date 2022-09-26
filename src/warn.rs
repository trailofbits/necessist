use crate::{LightContext, Span};
use ansi_term::{Color::Yellow, Style};
use lazy_static::lazy_static;
use std::{collections::BTreeSet, sync::Mutex};

#[derive(Eq, Ord, PartialEq, PartialOrd)]
#[remain::sorted]
pub(crate) enum Key {
    ConfigurationOrTestFilesHaveChanged,
    HardhatTsIgnoredMacros,
    RustIgnoredFunctions,
}

pub(crate) fn span_warn(context: &LightContext, span: &Span, msg: &str) {
    warn_internal(context, Some(span), msg, None);
}

pub(crate) fn warn(context: &LightContext, msg: &str) {
    warn_internal(context, None, msg, None);
}

pub(crate) fn warn_once(context: &LightContext, msg: &str, key: Key) {
    warn_internal(context, None, msg, Some(key));
}

lazy_static! {
    static ref KEYS_USED: Mutex<BTreeSet<Key>> = Mutex::new(BTreeSet::new());
}

fn warn_internal(context: &LightContext, span: Option<&Span>, msg: &str, key: Option<Key>) {
    if context.opts.quiet {
        return;
    }

    if let Some(key) = key {
        #[allow(clippy::unwrap_used)]
        let mut keys_used = KEYS_USED.lock().unwrap();
        if keys_used.contains(&key) {
            return;
        }
        keys_used.insert(key);
    }

    (context.println)(&format!(
        "{}{}: {}",
        span.map_or(String::new(), |span| format!(
            "{}: ",
            span.to_console_string()
        )),
        if atty::is(atty::Stream::Stdout) {
            Yellow.bold()
        } else {
            Style::default()
        }
        .paint("Warning"),
        msg
    ));
}
