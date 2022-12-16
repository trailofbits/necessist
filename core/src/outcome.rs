use ansi_term::{
    Color::{Blue, Green, Red, Yellow},
    Style,
};
use anyhow::{anyhow, Error};
use heck::ToKebabCase;
use std::str::FromStr;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(Clone, Copy, Debug, EnumIter, PartialEq)]
pub(crate) enum Outcome {
    Nonbuildable,
    Failed,
    TimedOut,
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
            .ok_or_else(|| anyhow!("Unknown outcome `{}`", s))
    }
}

impl Outcome {
    pub fn style(self) -> Style {
        match self {
            Outcome::Nonbuildable => Blue.normal(),
            Outcome::Failed => Green.normal(),
            Outcome::TimedOut => Yellow.normal(),
            Outcome::Passed => Red.normal(),
        }
    }
}
