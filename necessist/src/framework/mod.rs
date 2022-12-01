use anyhow::Result;
use clap::ValueEnum;
use heck::ToKebabCase;
use necessist_core::{implementation_as_interface, Interface, LightContext, ToImplementation};
use strum_macros::EnumIter;

mod impls;

#[derive(Debug, Clone, Copy, EnumIter, Eq, PartialEq, ValueEnum)]
#[remain::sorted]
pub(crate) enum Identifier {
    Foundry,
}

impl ToImplementation for Identifier {
    fn to_implementation(&self, context: &LightContext) -> Result<Option<Box<dyn Interface>>> {
        match self {
            Self::Foundry => impls::Foundry::applicable(context).map(implementation_as_interface),
        }
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{:?}", self).to_kebab_case())
    }
}
