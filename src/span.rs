use crate::{util, SourceFile};
use anyhow::{anyhow, Error};
use lazy_static::lazy_static;
use proc_macro2::LineColumn;
use regex::Regex;
use std::{path::Path, str::FromStr};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Span {
    pub source_file: SourceFile,
    pub start: LineColumn,
    pub end: LineColumn,
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_with_path(self.source_file.as_ref()))
    }
}

lazy_static! {
    static ref SPAN_RE: Regex = {
        #[allow(clippy::unwrap_used)]
        let re = Regex::new(r"^([^:]*):([^:]*):([^-]*)-([^:]*):(.*)$").unwrap();
        re
    };
}

impl FromStr for Span {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
            source_file: SourceFile::new(source_file),
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
}

impl Span {
    #[must_use]
    pub fn start(&self) -> LineColumn {
        self.start
    }

    #[must_use]
    pub fn end(&self) -> LineColumn {
        self.end
    }

    #[must_use]
    pub fn to_console_string(&self) -> String {
        self.to_string_with_path(util::strip_current_dir(&self.source_file))
    }

    fn to_string_with_path(&self, path: &Path) -> String {
        format!(
            "{}:{}:{}-{}:{}",
            path.to_string_lossy(),
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
