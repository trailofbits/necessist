// smoelius: This file is a slight modification of:
// https://github.com/smoelius/rustfmt_if_chain/blob/557c32c54b0e0f48da2d029a3a8f70db4c8dbf9b/src/offset_based_rewriter/impls.rs

use super::Interface;

#[derive(Debug)]
pub struct LazyRewriter<'original> {
    original: &'original str,
    rewritten: String,
    offset: usize,
}

impl<'original> LazyRewriter<'original> {
    pub fn new(original: &'original str) -> Self {
        Self {
            original,
            rewritten: String::new(),
            offset: 0,
        }
    }
}

impl<'original> Interface for LazyRewriter<'original> {
    fn contents(mut self) -> String {
        self.rewritten += &self.original[self.offset..];

        self.rewritten
    }

    fn rewrite(&mut self, start: usize, end: usize, replacement: &str) -> String {
        assert!(self.offset <= start);

        self.rewritten += &self.original[self.offset..start];
        self.rewritten += replacement;

        self.offset = end;

        String::from(&self.original[self.offset - (end - start)..self.offset])
    }
}
