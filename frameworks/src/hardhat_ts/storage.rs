use std::rc::Rc;
use swc_core::{common::SourceMap, ecma::ast::Module};

pub struct Storage<'ast> {
    pub source_map: &'ast Rc<SourceMap>,
}

impl<'ast> Storage<'ast> {
    pub fn new(file: &'ast (Rc<SourceMap>, Module)) -> Self {
        Self {
            source_map: &file.0,
        }
    }
}
