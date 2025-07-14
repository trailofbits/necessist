use super::{Interface, ToImplementation};
use crate::LightContext;
use anyhow::Result;
use std::marker::PhantomData;

#[cfg(feature = "clap")]
use clap::{ValueEnum, builder::PossibleValue};

// smoelius: The `IntoEnumIterator` trait and `Iter` enum are currently unused.
#[allow(dead_code)]
pub trait IntoEnumIterator: Sized {
    type Iterator: Iterator<Item = Self>;

    fn iter() -> Self::Iterator;
}

impl<T: strum::IntoEnumIterator> IntoEnumIterator for T {
    type Iterator = <Self as strum::IntoEnumIterator>::Iterator;

    fn iter() -> Self::Iterator {
        <Self as strum::IntoEnumIterator>::iter()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Union<L, R> {
    Left(L),
    Right(R),
}

#[allow(dead_code)]
pub enum Iter<L, R, I, J>
where
    L: IntoEnumIterator<Iterator = I>,
    R: IntoEnumIterator<Iterator = J>,
{
    Left(I, PhantomData<L>),
    Right(J, PhantomData<R>),
}

impl<L, R, I, J> Iter<L, R, I, J>
where
    L: IntoEnumIterator<Iterator = I>,
    R: IntoEnumIterator<Iterator = J>,
{
    fn new() -> Self {
        Self::Left(L::iter(), PhantomData)
    }
}

impl<L, R, I, J> Iterator for Iter<L, R, I, J>
where
    L: IntoEnumIterator<Iterator = I>,
    R: IntoEnumIterator<Iterator = J>,
    I: Iterator<Item = L>,
    J: Iterator<Item = R>,
{
    type Item = Union<L, R>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Self::Left(left, _) = self {
            if let Some(framework) = left.next() {
                return Some(Union::Left(framework));
            }
            *self = Self::Right(R::iter(), PhantomData);
        }
        if let Self::Right(right, _) = self {
            right.next().map(Union::Right)
        } else {
            unreachable!()
        }
    }
}

impl<L, R, I, J> IntoEnumIterator for Union<L, R>
where
    L: IntoEnumIterator<Iterator = I>,
    R: IntoEnumIterator<Iterator = J>,
    I: Iterator<Item = L>,
    J: Iterator<Item = R>,
{
    type Iterator = Iter<L, R, I, J>;

    fn iter() -> Self::Iterator {
        Iter::new()
    }
}

impl<L, R> ToImplementation for Union<L, R>
where
    L: ToImplementation,
    R: ToImplementation,
{
    fn to_implementation(&self, context: &LightContext) -> Result<Option<Box<dyn Interface>>> {
        match self {
            Self::Left(left) => left.to_implementation(context),
            Self::Right(right) => right.to_implementation(context),
        }
    }
}

#[cfg(feature = "clap")]
impl<L, R> ValueEnum for Union<L, R>
where
    L: Clone + ValueEnum,
    R: Clone + ValueEnum,
{
    fn value_variants<'a>() -> &'a [Self] {
        let mut names = L::value_variants()
            .iter()
            .filter_map(|left| {
                left.to_possible_value()
                    .map(|left| left.get_name().to_owned())
            })
            .collect::<Vec<_>>();
        names.extend(R::value_variants().iter().filter_map(|right| {
            right
                .to_possible_value()
                .map(|right| right.get_name().to_owned())
        }));
        names.sort();
        Box::leak(
            names
                .iter()
                .flat_map(|name| Self::from_str(name, false))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            Self::Left(left) => left.to_possible_value(),
            Self::Right(right) => right.to_possible_value(),
        }
    }

    fn from_str(input: &str, ignore_case: bool) -> Result<Self, String> {
        L::from_str(input, ignore_case)
            .map(Self::Left)
            .or_else(|left| {
                R::from_str(input, ignore_case)
                    .map(Self::Right)
                    .map_err(|right| format!("{left}, {right}"))
            })
    }
}
