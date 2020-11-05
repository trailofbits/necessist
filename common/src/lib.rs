use anyhow::Result;
use proc_macro2::{self, LineColumn};
use std::{
    env,
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub source_file: PathBuf,
    pub start: LineColumn,
    pub end: LineColumn,
}

impl Display for Span {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}-{}:{}",
            self.source_file.to_string_lossy(),
            self.start.line,
            self.start.column,
            self.end.line,
            self.end.column
        )
    }
}

impl Span {
    pub fn from_span(source_file: &Path, span: proc_macro2::Span) -> Self {
        Self {
            source_file: source_file.to_path_buf(),
            start: span.start(),
            end: span.end(),
        }
    }

    pub fn from_env() -> Result<Self> {
        Ok(Self {
            source_file: PathBuf::from(var_as_string("SOURCE_FILE")?),
            start: LineColumn {
                line: var_as_usize("START_LINE")?,
                column: var_as_usize("START_COLUMN")?,
            },
            end: LineColumn {
                line: var_as_usize("END_LINE")?,
                column: var_as_usize("END_COLUMN")?,
            },
        })
    }

    pub fn to_vec(&self) -> Vec<(String, String)> {
        vec![
            (
                key("SOURCE_FILE"),
                String::from(self.source_file.to_string_lossy()),
            ),
            (key("START_LINE"), self.start.line.to_string()),
            (key("START_COLUMN"), self.start.column.to_string()),
            (key("END_LINE"), self.end.line.to_string()),
            (key("END_COLUMN"), self.end.column.to_string()),
        ]
    }
}

fn key(suffix: &str) -> String {
    assert!(!suffix.is_empty());
    "NECESSIST_".to_owned() + suffix
}

fn var_as_string(suffix: &str) -> Result<String> {
    Ok(env::var(key(suffix))?)
}

fn var_as_usize(suffix: &str) -> Result<usize> {
    Ok(var_as_string(suffix)?.parse::<usize>()?)
}

pub fn removed_message(span: &Span) -> String {
    format!("{}: removed", span)
}
