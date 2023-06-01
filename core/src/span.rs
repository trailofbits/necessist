use crate::{Backup, Rewriter, SourceFile, ToConsoleString};
use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use regex::Regex;
use std::{fs::OpenOptions, io::Write, path::PathBuf, rc::Rc};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Span {
    pub source_file: SourceFile,
    pub start: proc_macro2::LineColumn,
    pub end: proc_macro2::LineColumn,
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
        let source_file = SourceFile::new(root.clone(), root.join(source_file))?;
        Ok(Self {
            source_file,
            start: proc_macro2::LineColumn {
                line: start_line,
                column: start_column - 1,
            },
            end: proc_macro2::LineColumn {
                line: end_line,
                column: end_column - 1,
            },
        })
    }

    #[must_use]
    pub fn start(&self) -> proc_macro2::LineColumn {
        self.start
    }

    #[must_use]
    pub fn end(&self) -> proc_macro2::LineColumn {
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

    #[must_use]
    pub fn trim_start(&self) -> Self {
        // smoelius: Ignoring errors is a hack.
        let Ok(text) = self.source_text() else {
            return self.clone();
        };

        let mut start = self.start;
        for ch in text.chars() {
            if ch.is_whitespace() {
                if ch == '\n' {
                    start.line += 1;
                    start.column = 0;
                } else {
                    start.column += 1;
                }
            } else {
                break;
            }
        }

        self.with_start(start)
    }

    #[must_use]
    pub fn with_start(&self, start: proc_macro2::LineColumn) -> Self {
        Self {
            source_file: self.source_file.clone(),
            start,
            end: self.end,
        }
    }

    pub fn source_text(&self) -> Result<String> {
        let contents = self.source_file.contents();

        // smoelius: Creating a new `Rewriter` here is just as silly as it is in `attempt_removal`
        // (see comment therein).
        // smoelius: `Rewriter`s are now cheap to create because their underlying
        // `OffsetCalculator`s are shared.
        let rewriter = Rewriter::new(contents, self.source_file.offset_calculator());
        let (start, end) = rewriter.offsets_from_span(self);

        let bytes = &contents.as_bytes()[start..end];
        let text = std::str::from_utf8(bytes)?;

        Ok(text.to_owned())
    }

    pub fn remove(&self) -> Result<(String, Backup)> {
        let backup = Backup::new(&*self.source_file)?;

        let mut rewriter = Rewriter::new(
            self.source_file.contents(),
            self.source_file.offset_calculator(),
        );

        let text = rewriter.rewrite(self, "");

        let mut file = OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(&*self.source_file)?;
        file.write_all(rewriter.contents().as_bytes())?;

        Ok((text, backup))
    }
}

#[allow(clippy::module_name_repetitions)]
pub trait ToInternalSpan {
    fn to_internal_span(&self, source_file: &SourceFile) -> Span;
}

impl ToInternalSpan for proc_macro2::Span {
    fn to_internal_span(&self, source_file: &SourceFile) -> Span {
        Span {
            source_file: source_file.clone(),
            start: self.start(),
            end: self.end(),
        }
    }
}
