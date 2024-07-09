// smoelius: This file is a slight modification of:
// https://github.com/smoelius/rustfmt_if_chain/blob/557c32c54b0e0f48da2d029a3a8f70db4c8dbf9b/src/offset_based_rewriter/mod.rs

mod impls;

use impls::LazyRewriter;

pub trait Interface {
    fn contents(self) -> String;
    fn rewrite(&mut self, start: usize, end: usize, replacement: &str) -> String;
}

#[derive(Debug)]
pub struct OffsetBasedRewriter<'original> {
    lazy: LazyRewriter<'original>,
}

impl<'original> OffsetBasedRewriter<'original> {
    pub fn new(original: &'original str) -> Self {
        Self {
            lazy: LazyRewriter::new(original),
        }
    }
}

impl<'original> Interface for OffsetBasedRewriter<'original> {
    #[allow(clippy::let_and_return)]
    fn contents(self) -> String {
        let contents = self.lazy.contents();

        contents
    }

    #[allow(clippy::let_and_return)]
    fn rewrite(&mut self, start: usize, end: usize, replacement: &str) -> String {
        let replaced = self.lazy.rewrite(start, end, replacement);

        replaced
    }
}
