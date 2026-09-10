use crate::{__ToConsoleString as ToConsoleString, Backup, Rewriter, SourceFile};
use anyhow::{Context, Result, anyhow};
use elaborate::std::{fs::OpenOptionsContext, io::WriteContext};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, path::PathBuf, rc::Rc, sync::LazyLock};

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
            .ok_or_else(|| anyhow!("span has unexpected format"))?;
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

    /// Returns the spanned text, or an error if the span's offsets are not a valid range within
    /// the contents
    ///
    /// The contents are cached per path while each backend rereads the file, so even a span just
    /// produced by parsing can fall outside them. An `Ok` is not proof that the span is current
    /// either: an overlong column is clamped rather than rejected.
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

        let text = contents.get(start..end).ok_or_else(|| {
            anyhow!(
                "`{}..{}` is not a valid range within the contents of `{}`; the file may have \
                 changed since it was read",
                start,
                end,
                self.source_file.display()
            )
        })?;

        Ok(text.to_owned())
    }

    pub fn remove(&self) -> Result<(String, Backup)> {
        let backup = Backup::new(&*self.source_file)
            .with_context(|| format!("failed to backup `{}`", self.source_file.display()))?;

        let mut rewriter = Rewriter::with_offset_calculator(
            self.source_file.contents(),
            self.source_file.offset_calculator(),
        );

        let text = rewriter.rewrite(self, "");

        let mut file = OpenOptions::new()
            .truncate(true)
            .write(true)
            .open_wc(&*self.source_file)?;
        file.write_all_wc(rewriter.contents().as_bytes())?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use elaborate::std::fs::write_wc;
    use testing::tempfile_util::tempdir;

    #[test]
    fn source_text_returns_the_text_of_a_span_within_the_contents() {
        let tempdir = tempdir().unwrap();
        let root = Rc::new(tempdir.path().to_path_buf());
        write_wc(root.join("f.rs"), "aaaa\nbb\n").unwrap();

        let span = Span::parse(&root, "f.rs:1:1-1:3").unwrap();

        assert_eq!("aa", span.source_text().unwrap());
    }

    #[test]
    fn source_text_returns_an_error_for_an_inverted_span() {
        let tempdir = tempdir().unwrap();
        let root = Rc::new(tempdir.path().to_path_buf());
        write_wc(root.join("f.rs"), "aaaa\nbb\n").unwrap();

        // Ends before it starts. The offsets are both within the contents, so a check that looked
        // only at the file's length would let this through and then index with `start > end`.
        let span = Span::parse(&root, "f.rs:2:2-1:2").unwrap();

        let error = span.source_text().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("is not a valid range within the contents of"),
            "{error}"
        );
    }

    #[test]
    fn source_text_returns_an_error_for_a_span_exceeding_the_contents() {
        let tempdir = tempdir().unwrap();
        let root = Rc::new(tempdir.path().to_path_buf());
        // No trailing newline: it is what makes the computed offset exceed the file's length.
        write_wc(root.join("f.rs"), "aaaa\nbb").unwrap();

        let span = Span::parse(&root, "f.rs:3:1-3:5").unwrap();

        // Before this change, indexing the contents at this offset panicked.
        let error = span.source_text().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("is not a valid range within the contents of"),
            "{error}"
        );
    }
}
