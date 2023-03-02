use anyhow::Result;
use clap::ValueEnum;
use heck::ToKebabCase;
use necessist_core::{
    framework::{Interface, ToImplementation},
    LightContext,
};
use strum_macros::EnumIter;

#[derive(Debug, Clone, Copy, EnumIter, Eq, PartialEq, ValueEnum)]
#[remain::sorted]
pub(crate) enum Identifier {}

impl ToImplementation for Identifier {
    fn to_implementation(&self, _context: &LightContext) -> Result<Option<Box<dyn Interface>>> {
        match *self {}
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_kebab_case())
    }
}
