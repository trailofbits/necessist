#![warn(clippy::expect_used)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::panic)]

pub use proc_macro2::LineColumn;

mod backup {
    pub use rewriter::Backup;
}
use backup::Backup;

#[doc(hidden)]
pub use backup::Backup as __Backup;

#[cfg(feature = "clap")]
pub mod cli;

pub mod config;

mod core;
use crate::core::Removal;
pub use crate::core::{LightContext, Necessist, necessist};

#[cfg(feature = "lock_root")]
mod flock;

pub mod framework;
mod offset_calculator {
    pub type OffsetCalculator<'original> = rewriter::OffsetCalculator<'original, crate::Span>;
}

mod outcome;
use outcome::Outcome;

mod rewriter {
    pub type Rewriter<'original> = rewriter::Rewriter<'original, crate::Span>;
}
use rewriter::Rewriter;
// smoelius: `Rewriter` is used by the Go framework to convert byte offsets to char offsets.
#[doc(hidden)]
pub use rewriter::Rewriter as __Rewriter;

mod source_file;
pub use source_file::SourceFile;

mod span;
pub use span::{Span, ToInternalSpan};

mod sqlite;

mod to_console_string;
#[doc(hidden)]
pub use to_console_string::ToConsoleString as __ToConsoleString;

pub mod util;

mod warn;
use warn::note;
pub use warn::{Flags as WarnFlags, Warning, source_warn, warn};
