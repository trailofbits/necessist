use crate::{__ToConsoleString as ToConsoleString, Backup, Rewriter, SourceFile};
use anyhow::{Result, anyhow};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, io::Write, path::PathBuf, rc::Rc, sync::LazyLock};

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

impl rewriter::interface::Span for Span {
    type LineColumn = proc_macro2::LineColumn;
    fn line_column(line: usize, column: usize) -> Self::LineColumn {
        proc_macro2::LineColumn { line, column }
    }
    fn start(&self) -> Self::LineColumn {
        self.start
    }
    fn end(&self) -> Self::LineColumn {
        self.end
    }
}

impl ToConsoleString for Span {
    fn to_console_string(&self) -> String {
        self.to_string_with_path(&self.source_file.to_console_string())
    }
}

static SPAN_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::unwrap_used)]
    Regex::new(r"^([^:]*):([^:]*):([^-]*)-([^:]*):(.*)$").unwrap()
});

impl Span {
    #[must_use]
    pub fn id(&self) -> String {
        const ID_LEN: usize = 16;
        let mut hasher = Sha256::new();
        hasher.update(self.to_string());
        let digest = hasher.finalize();
        hex::encode(digest)[..ID_LEN].to_owned()
    }

    pub fn parse(root: &Rc<PathBuf>, s: &str) -> Result<Self> {
        let (source_file, start_line, start_column, end_line, end_column) = SPAN_RE
            .captures(s)
            .map(|captures| {
                assert_eq!(6, captures.len());
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
    pub fn source_file(&self) -> SourceFile {
        self.source_file.clone()
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

    /// Returns the spanned text.
    pub fn source_text(&self) -> Result<String> {
        let contents = self.source_file.contents();

        // smoelius: Creating a new `Rewriter` here is just as silly as it is in `attempt_removal`
        // (see comment therein).
        // smoelius: `Rewriter`s are now cheap to create because their underlying
        // `OffsetCalculator`s are shared.
        let (start, end) = self
            .source_file
            .offset_calculator()
            .borrow_mut()
            .offsets_from_span(self);

        let bytes = &contents.as_bytes()[start..end];
        let text = std::str::from_utf8(bytes)?;

        Ok(text.to_owned())
    }

    pub fn remove(&self) -> Result<(String, Backup)> {
        let backup = Backup::new(&*self.source_file)?;

        let mut rewriter = Rewriter::with_offset_calculator(
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
