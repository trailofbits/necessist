use crate::{SourceFile, ToConsoleString};
use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use proc_macro2::LineColumn;
use regex::Regex;
use std::{path::PathBuf, rc::Rc};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Span {
    pub source_file: SourceFile,
    pub start: LineColumn,
    pub end: LineColumn,
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // smoelius: `source_file.to_string()` gives the path relative to the project root.
        write!(
            f,
            "{}",
            self.to_string_with_path(&self.source_file.to_string())
        )
    }
}

impl ToConsoleString for Span {
    fn to_console_string(&self) -> String {
        self.to_string_with_path(&self.source_file.to_console_string())
    }
}

lazy_static! {
    static ref SPAN_RE: Regex = {
        #[allow(clippy::unwrap_used)]
        Regex::new(r"^([^:]*):([^:]*):([^-]*)-([^:]*):(.*)$").unwrap()
    };
}

impl Span {
    pub fn parse(root: &Rc<PathBuf>, s: &str) -> Result<Self> {
        let (source_file, start_line, start_column, end_line, end_column) = SPAN_RE
            .captures(s)
            .map(|captures| {
                assert!(captures.len() == 6);
                (
                    captures[1].to_owned(),
                    captures[2].to_owned(),
                    captures[3].to_owned(),
                    captures[4].to_owned(),
                    captures[5].to_owned(),
                )
            })
            .ok_or_else(|| anyhow!("Span has unexpected format"))?;
        let start_line = start_line.parse::<usize>()?;
        let start_column = start_column.parse::<usize>()?;
        let end_line = end_line.parse::<usize>()?;
        let end_column = end_column.parse::<usize>()?;
        Ok(Self {
            source_file: SourceFile::new(root.clone(), Rc::new(root.join(source_file))),
            start: LineColumn {
                line: start_line,
                column: start_column - 1,
            },
            end: LineColumn {
                line: end_line,
                column: end_column - 1,
            },
        })
    }

    #[must_use]
    pub fn start(&self) -> LineColumn {
        self.start
    }

    #[must_use]
    pub fn end(&self) -> LineColumn {
        self.end
    }

    fn to_string_with_path(&self, path: &str) -> String {
        format!(
            "{}:{}:{}-{}:{}",
            path,
            self.start.line,
            self.start.column + 1,
            self.end.line,
            self.end.column + 1
        )
    }
}

pub(crate) trait ToInternalSpan {
    fn to_internal_span(&self, source_file: &SourceFile) -> Span;
}
