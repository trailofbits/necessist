// smoelius: This file is a slight modification of:
// https://github.com/smoelius/rustfmt_if_chain/blob/557c32c54b0e0f48da2d029a3a8f70db4c8dbf9b/src/offset_calculator/impls.rs

#![allow(clippy::unwrap_used)]

use super::Interface;
use proc_macro2::LineColumn;
use std::str::{Chars, Split};

#[derive(Debug)]
pub struct CachingOffsetCalculator<'original> {
    lines: Split<'original, char>,
    chars: Option<Chars<'original>>,
    line_column: LineColumn,
    offset: usize,
    ascii: bool,
}

#[derive(Debug)]
pub struct StatelessOffsetCalculator<'original> {
    original: &'original str,
}

impl<'original> CachingOffsetCalculator<'original> {
    pub fn new(original: &'original str) -> Self {
        Self {
            lines: original.split('\n'),
            chars: None,
            line_column: LineColumn { line: 1, column: 0 },
            offset: 0,
            ascii: true,
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
        assert!(self.line_column <= line_column);

        let mut chars = self
            .chars
            .take()
            .unwrap_or_else(|| self.lines.next().unwrap().chars());

        if self.line_column.line < line_column.line {
            let suffix = chars.collect::<String>();
            self.offset += suffix.as_bytes().len() + 1;
            self.ascii &= suffix.chars().all(|ch| ch.is_ascii());
            self.line_column.line += 1;
            self.line_column.column = 0;

            while self.line_column.line < line_column.line {
                let line = self.lines.next().unwrap();
                self.offset += line.as_bytes().len() + 1;
                self.ascii &= line.chars().all(|ch| ch.is_ascii());
                self.line_column.line += 1;
                self.line_column.column = 0;
            }

            chars = self.lines.next().unwrap().chars();
        }

        let prefix = (&mut chars)
            .take(line_column.column - self.line_column.column)
            .collect::<String>();
        self.offset += prefix.as_bytes().len();
        self.ascii &= prefix.chars().all(|ch| ch.is_ascii());
        self.line_column.column = line_column.column;

        self.chars = Some(chars);

        (self.offset, self.ascii)
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
