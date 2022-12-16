use crate::{Config, LightContext, Span};
use anyhow::Result;
use heck::ToKebabCase;
use std::{any::type_name, path::Path};
use strum_macros::EnumIter;
use subprocess::{Exec, Popen};

mod auto;
pub use auto::Auto;

mod empty;
pub use empty::Empty;

mod impls;

mod union;
pub use union::Union;

pub type AutoUnion<T, U> = Auto<Union<T, U>>;

pub type Postprocess = dyn Fn(&LightContext, Popen) -> Result<bool>;

pub trait Interface: std::fmt::Debug {
    #[allow(clippy::unwrap_used)]
    fn name(&self) -> String {
        let (_, type_name) = type_name::<Self>().rsplit_once("::").unwrap();
        type_name.to_kebab_case()
    }
    fn parse(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>>;
    fn dry_run(&self, context: &LightContext, test_file: &Path) -> Result<()>;
    fn exec(
        &self,
        context: &LightContext,
        span: &Span,
    ) -> Result<Option<(Exec, Option<Box<Postprocess>>)>>;
}

/// Utility function
pub fn implementation_as_interface(
    implementation: Option<impl Interface + 'static>,
) -> Option<Box<dyn Interface>> {
    implementation.map(|implementation| Box::new(implementation) as Box<dyn Interface>)
}

pub trait ToImplementation {
    fn to_implementation(&self, context: &LightContext) -> Result<Option<Box<dyn Interface>>>;
}

#[derive(Debug, Clone, Copy, EnumIter, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[non_exhaustive]
#[remain::sorted]
pub enum Identifier {
    HardhatTs,
    Rust,
}

impl ToImplementation for Identifier {
    fn to_implementation(&self, context: &LightContext) -> Result<Option<Box<dyn Interface>>> {
        match self {
            Self::HardhatTs => {
                impls::HardhatTs::applicable(context).map(implementation_as_interface)
            }
            Self::Rust => impls::Rust::applicable(context).map(implementation_as_interface),
        }
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_kebab_case())
    }
}
