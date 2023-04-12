use super::ParseHigh;
use anyhow::Result;
use necessist_core::{Config, LightContext, Span};
use std::{cell::RefCell, path::Path, rc::Rc};

pub trait ParseLow {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>>;
}

impl<T: ParseLow> ParseLow for Rc<RefCell<T>> {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>> {
        self.borrow_mut().parse(context, config, test_files)
    }
}

pub struct ParseAdapter<T>(pub T);

impl<T: ParseLow> ParseHigh for ParseAdapter<T> {
    fn parse(
        &mut self,
        context: &LightContext,
        config: &Config,
        test_files: &[&Path],
    ) -> Result<Vec<Span>> {
        self.0.parse(context, config, test_files)
    }
}
