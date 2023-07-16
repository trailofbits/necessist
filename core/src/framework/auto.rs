use super::{Applicable, Interface, ToImplementation, Union};
use crate::LightContext;
use anyhow::{ensure, Result};
use std::fmt::Display;
use strum::IntoEnumIterator;

#[cfg(feature = "clap")]
use clap::{builder::PossibleValue, ValueEnum};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
enum Singleton {
    Auto,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Auto<T>(Union<Singleton, T>);

impl<T> Default for Auto<T> {
    fn default() -> Self {
        Self(Union::Left(Singleton::Auto))
    }
}

impl<T> ToImplementation for Auto<T>
where
    T: Applicable + Display + IntoEnumIterator + ToImplementation,
{
    fn to_implementation(&self, context: &LightContext) -> Result<Option<Box<dyn Interface>>> {
        match &self.0 {
            Union::Left(_) => {
                let unflattened_frameworks = T::iter()
                    .map(|framework| {
                        if framework.applicable(context)? {
                            Ok(Some(framework))
                        } else {
                            Ok(None)
                        }
                    })
                    .collect::<Result<Vec<_>>>()?;

                let applicable_frameworks = unflattened_frameworks
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();

                ensure!(
                    applicable_frameworks.len() <= 1,
                    "Found multiple applicable frameworks: {}; please select one with --framework \
                     <FRAMEWORK>",
                    applicable_frameworks
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                );

                if let Some(framework) = applicable_frameworks.into_iter().next() {
                    framework.to_implementation(context)
                } else {
                    Ok(None)
                }
            }
            Union::Right(framework) => framework.to_implementation(context),
        }
    }
}

#[cfg(feature = "clap")]
impl<T> ValueEnum for Auto<T>
where
    T: Clone + ValueEnum,
{
    fn value_variants<'a>() -> &'a [Self] {
        Box::leak(
            Union::<Singleton, T>::value_variants()
                .iter()
                .cloned()
                .map(Self)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        self.0.to_possible_value()
    }

    fn from_str(input: &str, ignore_case: bool) -> Result<Self, String> {
        Union::<Singleton, T>::from_str(input, ignore_case).map(Self)
    }
}
