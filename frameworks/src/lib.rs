use anyhow::Result;
use clap::ValueEnum;
use heck::ToKebabCase;
use necessist_core::{
    framework::{Applicable, Interface, ToImplementation},
    LightContext,
};
use strum_macros::EnumIter;

mod foundry;
use foundry::Foundry;

mod golang;
use golang::Golang;

mod hardhat_ts;
use hardhat_ts::HardhatTs;

mod rust;
use rust::Rust;

mod ts_utils;

#[derive(Debug, Clone, Copy, EnumIter, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[non_exhaustive]
#[remain::sorted]
pub enum Identifier {
    Foundry,
    Golang,
    HardhatTs,
    Rust,
}

impl Applicable for Identifier {
    fn applicable(&self, context: &LightContext) -> Result<bool> {
        match *self {
            Self::Foundry => Foundry::applicable(context),
            Self::Golang => Golang::applicable(context),
            Self::HardhatTs => HardhatTs::applicable(context),
            Self::Rust => Rust::applicable(context),
        }
    }
}

impl ToImplementation for Identifier {
    fn to_implementation(&self, context: &LightContext) -> Result<Option<Box<dyn Interface>>> {
        Ok(Some(match *self {
            Self::Foundry => implementation_as_interface(Foundry::new(context)),
            Self::Golang => implementation_as_interface(Golang::new(context)),
            Self::HardhatTs => implementation_as_interface(HardhatTs::new(context)),
            Self::Rust => implementation_as_interface(Rust::new(context)),
        }))
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_kebab_case())
    }
}

/// Utility function
fn implementation_as_interface(implementation: impl Interface + 'static) -> Box<dyn Interface> {
    Box::new(implementation) as Box<dyn Interface>
}
