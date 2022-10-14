use super::{Interface, ToImplementation};
use crate::LightContext;
use anyhow::Result;
use strum_macros::EnumIter;

#[derive(Debug, Clone, Copy, EnumIter, Eq, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum Empty {}

impl ToImplementation for Empty {
    fn to_implementation(&self, _context: &LightContext) -> Result<Option<Box<dyn Interface>>> {
        Ok(None)
    }
}
