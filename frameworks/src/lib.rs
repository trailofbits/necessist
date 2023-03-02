use anyhow::Result;
use clap::ValueEnum;
use heck::ToKebabCase;
use necessist_core::{
    framework::{Interface, ToImplementation},
    LightContext,
};
use strum_macros::EnumIter;

mod foundry;
use foundry::Foundry;

mod hardhat_ts;
use hardhat_ts::HardhatTs;

mod rust;
use rust::Rust;

#[derive(Debug, Clone, Copy, EnumIter, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[non_exhaustive]
#[remain::sorted]
pub enum Identifier {
    Foundry,
    HardhatTs,
    Rust,
}

impl ToImplementation for Identifier {
    fn to_implementation(&self, context: &LightContext) -> Result<Option<Box<dyn Interface>>> {
        match *self {
            Self::Foundry => Foundry::applicable(context).map(implementation_as_interface),
            Self::HardhatTs => HardhatTs::applicable(context).map(implementation_as_interface),
            Self::Rust => Rust::applicable(context).map(implementation_as_interface),
        }
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_kebab_case())
    }
}

/// Utility function
fn implementation_as_interface(
    implementation: Option<impl Interface + 'static>,
) -> Option<Box<dyn Interface>> {
    implementation.map(|implementation| Box::new(implementation) as Box<dyn Interface>)
}
