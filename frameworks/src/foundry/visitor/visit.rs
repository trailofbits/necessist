#![allow(
    clippy::match_same_arms,
    clippy::module_name_repetitions,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]

#[allow(clippy::wildcard_imports)]
use solang_parser::pt::*;

// smoelius: `Visitable` is based on a trait with the same name in:
// https://github.com/foundry-rs/foundry/blob/8b6d36f0ba93548be4b332bf9183a463447559f6/fmt/src/visit.rs

pub trait Visitable {
    fn visit<'ast, V>(&'ast self, v: &mut V) -> Result<(), V::Error>
    where
        V: Visitor<'ast> + ?Sized;
}

impl<T> Visitable for Option<T>
where
    T: Visitable,
{
    fn visit<'ast, V>(&'ast self, v: &mut V) -> Result<(), V::Error>
    where
        V: Visitor<'ast> + ?Sized,
    {
        if let Some(inner) = self {
            inner.visit(v)
        } else {
            Ok(())
        }
    }
}

impl<T> Visitable for Box<T>
where
    T: Visitable,
{
    fn visit<'ast, V>(&'ast self, v: &mut V) -> Result<(), V::Error>
    where
        V: Visitor<'ast> + ?Sized,
    {
        T::visit(self, v)
    }
}

impl<T> Visitable for Vec<T>
where
    T: Visitable,
{
    fn visit<'ast, V>(&'ast self, v: &mut V) -> Result<(), V::Error>
    where
        V: Visitor<'ast> + ?Sized,
    {
        for item in self {
            item.visit(v)?;
        }
        Ok(())
    }
}

impl<T, U> Visitable for (T, U)
where
    T: Visitable,
    U: Visitable,
{
    fn visit<'ast, V>(&'ast self, v: &mut V) -> Result<(), V::Error>
    where
        V: Visitor<'ast> + ?Sized,
    {
        self.0.visit(v)?;
        self.1.visit(v)?;
        Ok(())
    }
}

macro_rules! impl_noop_visitable {
    ($ty:ty) => {
        impl Visitable for $ty {
            fn visit<'ast, V>(&'ast self, _: &mut V) -> Result<(), V::Error>
            where
                V: Visitor<'ast> + ?Sized,
            {
                Ok(())
            }
        }
    };
}

impl_noop_visitable!(bool);
impl_noop_visitable!(u8);
impl_noop_visitable!(u16);
impl_noop_visitable!(usize);
impl_noop_visitable!(String);

include!(concat!(env!("OUT_DIR"), "/visit.rs"));
