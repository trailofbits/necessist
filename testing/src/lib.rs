#![cfg_attr(
    dylint_lib = "supplementary",
    allow(
        crate_wide_allow,
        nonexistent_path_in_comment,
        reason = "`/private/tmp` exists on macOS but not Linux"
    )
)]

pub mod end_to_end_util;
pub mod tempfile_util;
