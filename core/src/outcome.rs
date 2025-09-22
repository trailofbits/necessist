use ansi_term::{
    Color::{Blue, Green, Red, Yellow},
    Style,
};
use anyhow::{Error, anyhow};
use heck::ToKebabCase;
use std::str::FromStr;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

/// The outcome of running a test with a statement or method call removed.
#[derive(Clone, Copy, Debug, EnumIter, PartialEq)]
pub(crate) enum Outcome {
    // The test(s) were not run (e.g., because a dry run failed).
    Skipped,
    /// The test(s) did not build.
    Nonbuildable,
    /// The test(s) built but failed.
    Failed,
    /// The test(s) built but timed-out.
    TimedOut,
    // The test(s) built and passed.
    Passed,
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_kebab_case())
    }
}

impl FromStr for Outcome {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Outcome::iter()
            .find(|outcome| outcome.to_string() == s)
            .ok_or_else(|| anyhow!("Unknown outcome `{s}`"))
    }
}

impl Outcome {
    pub fn style(self) -> Style {
        match self {
            Outcome::Skipped => Style::default().dimmed(),
            Outcome::Nonbuildable => Blue.normal(),
            Outcome::Failed => Green.normal(),
            Outcome::TimedOut => Yellow.normal(),
            Outcome::Passed => Red.normal(),
        }
    }
}
