#![cfg_attr(
    dylint_lib = "supplementary",
    allow(
        crate_wide_allow,
        nonexistent_path_in_comment,
        reason = "`/private/tmp` exists on macOS but not Linux"
    )
)]

pub mod tempfile_util;
