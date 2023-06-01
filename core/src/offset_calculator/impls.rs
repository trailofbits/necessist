// smoelius: This file is a slight modification of:
// https://github.com/smoelius/rustfmt_if_chain/blob/557c32c54b0e0f48da2d029a3a8f70db4c8dbf9b/src/offset_calculator/impls.rs

#![allow(clippy::unwrap_used)]

use super::Interface;
use proc_macro2::LineColumn;
use std::str::{Chars, Split};

#[derive(Debug)]
pub struct CachingOffsetCalculator<'original> {
    lines: Split<'original, char>,
    prefix: String,
    chars: Option<Chars<'original>>,
    line_column: LineColumn,
    offset: usize,
    ascii: bool,
    earliest_non_ascii_zero_based_index: usize,
    // smoelius: The offset is where the line ends, just past the newline.
    lines_and_offsets: Vec<(String, usize)>,
}

#[derive(Debug)]
pub struct StatelessOffsetCalculator<'original> {
    original: &'original str,
}

impl<'original> CachingOffsetCalculator<'original> {
    pub fn new(original: &'original str) -> Self {
        Self {
            lines: original.split('\n'),
            prefix: String::new(),
            chars: None,
            line_column: LineColumn { line: 1, column: 0 },
            offset: 0,
            ascii: true,
            earliest_non_ascii_zero_based_index: 0,
            lines_and_offsets: Vec::new(),
        }
    }
}

impl<'original> StatelessOffsetCalculator<'original> {
    #[allow(dead_code)]
    pub fn new(original: &'original str) -> Self {
        Self { original }
    }
}

impl<'original> Interface for CachingOffsetCalculator<'original> {
    fn offset_from_line_column(&mut self, line_column: LineColumn) -> (usize, bool) {
        assert!(self.line_column.line == self.lines_and_offsets.len() + 1);

        if line_column < self.line_column {
            let prefix = if line_column.line < self.line_column.line {
                &self.lines_and_offsets[line_column.line - 1].0
            } else {
                &self.prefix
            };
            let line_offset = if line_column.line <= 1 {
                0
            } else {
                self.lines_and_offsets[line_column.line - 2].1
            };
            let (_, char_offset, prefix_is_ascii) =
                Self::char_offset(&mut prefix.chars(), line_column.column);
            let offset = line_offset + char_offset;
            let ascii = (line_column.line - 1) < self.earliest_non_ascii_zero_based_index
                || ((line_column.line - 1) == self.earliest_non_ascii_zero_based_index
                    && prefix_is_ascii);
            return (offset, ascii);
        }

        let mut chars = self
            .chars
            .take()
            .unwrap_or_else(|| self.lines.next().unwrap().chars());

        if self.line_column.line < line_column.line {
            self.push_lines(chars, line_column.line);

            chars = self.lines.next().unwrap().chars();
        }

        assert!(self.line_column.line >= line_column.line);

        let (prefix, offset, ascii) =
            Self::char_offset(&mut chars, line_column.column - self.line_column.column);
        self.prefix += &prefix;
        self.chars = Some(chars);
        self.offset += offset;
        self.ascii &= ascii;

        self.line_column.column = line_column.column;

        (self.offset, self.ascii)
    }
}

impl<'original> CachingOffsetCalculator<'original> {
    fn push_lines(&mut self, chars: Chars<'_>, one_based_index: usize) {
        let suffix = chars.collect::<String>();

        self.offset += suffix.as_bytes().len() + 1;
        self.ascii &= suffix.chars().all(|ch| ch.is_ascii());
        self.line_column.line += 1;
        self.line_column.column = 0;

        let line = self.prefix.split_off(0) + &suffix;

        if self.earliest_non_ascii_zero_based_index >= self.lines_and_offsets.len() && self.ascii {
            self.earliest_non_ascii_zero_based_index += 1;
        }
        self.lines_and_offsets.push((line, self.offset));

        while self.line_column.line < one_based_index {
            let line = self.lines.next().unwrap();

            self.offset += line.as_bytes().len() + 1;
            self.ascii &= line.chars().all(|ch| ch.is_ascii());
            self.line_column.line += 1;
            self.line_column.column = 0;

            if self.earliest_non_ascii_zero_based_index >= self.lines_and_offsets.len()
                && self.ascii
            {
                self.earliest_non_ascii_zero_based_index += 1;
            }
            self.lines_and_offsets.push((line.to_owned(), self.offset));
        }
    }

    fn char_offset(chars: &mut Chars<'_>, column: usize) -> (String, usize, bool) {
        let prefix = chars.take(column).collect::<String>();
        let offset = prefix.as_bytes().len();
        let ascii = prefix.chars().all(|ch| ch.is_ascii());
        (prefix, offset, ascii)
    }
}

impl<'original> Interface for StatelessOffsetCalculator<'original> {
    #[cfg_attr(
        dylint_lib = "misleading_variable_name",
        allow(misleading_variable_name)
    )]
    fn offset_from_line_column(&mut self, line_column: LineColumn) -> (usize, bool) {
        let mut lines = self.original.split('\n');
        let mut offset = 0;
        let mut ascii = true;

        for _ in 1..line_column.line {
            let line = lines.next().unwrap();
            offset += line.as_bytes().len() + 1;
            ascii &= line.chars().all(|ch| ch.is_ascii());
        }

        let prefix = lines
            .next()
            .unwrap()
            .chars()
            .take(line_column.column)
            .collect::<String>();
        offset += prefix.as_bytes().len();
        ascii &= prefix.chars().all(|ch| ch.is_ascii());

        (offset, ascii)
    }
}
