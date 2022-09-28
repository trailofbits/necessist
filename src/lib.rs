#![warn(clippy::expect_used)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]

pub use proc_macro2::LineColumn;

mod backup;
use backup::Backup;

mod core;
pub use crate::core::{necessist, Framework, Necessist};
use crate::core::{Config, LightContext, Removal};

mod frameworks;
use frameworks::frameworks;

mod offset_based_rewriter;

mod offset_calculator;

mod outcome;
use outcome::Outcome;

mod rewriter;
use rewriter::Rewriter;

mod source_file;
use source_file::SourceFile;

mod span;
pub use span::Span;
use span::ToInternalSpan;

mod sqlite;

mod to_console_string;
use to_console_string::ToConsoleString;

mod try_insert;
use try_insert::TryInsert;

pub mod util;

mod warn;
pub use warn::Warning;
use warn::{note, source_warn, warn, warn_once};
