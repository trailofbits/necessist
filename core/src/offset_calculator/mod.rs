// smoelius: This file is a slight modification of:
// https://github.com/smoelius/rustfmt_if_chain/blob/557c32c54b0e0f48da2d029a3a8f70db4c8dbf9b/src/offset_calculator/mod.rs

use proc_macro2::LineColumn;

mod impls;

pub trait Interface {
    fn offset_from_line_column(&mut self, line_column: LineColumn) -> (usize, bool);
}

#[derive(Debug)]
pub struct OffsetCalculator<'original> {
    caching: impls::CachingOffsetCalculator<'original>,

    #[cfg(feature = "check-offsets")]
    stateless: impls::StatelessOffsetCalculator<'original>,
}

impl<'original> OffsetCalculator<'original> {
    pub fn new(original: &'original str) -> Self {
        Self {
            caching: impls::CachingOffsetCalculator::new(original),

            #[cfg(feature = "check-offsets")]
            stateless: impls::StatelessOffsetCalculator::new(original),
        }
    }
}

impl<'original> Interface for OffsetCalculator<'original> {
    fn offset_from_line_column(&mut self, line_column: LineColumn) -> (usize, bool) {
        let (offset, ascii) = self.caching.offset_from_line_column(line_column);

        #[cfg(feature = "check-offsets")]
        {
            let (offset_comparator, ascii_comparator) =
                self.stateless.offset_from_line_column(line_column);
            assert_eq!(offset, offset_comparator);
            assert_eq!(ascii, ascii_comparator);
        }

        (offset, ascii)
    }
}
