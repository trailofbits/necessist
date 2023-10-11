#! /bin/bash

# set -x
set -euo pipefail

# smoelius: The next command should match the `clippy` test in necessist/tests/ci.rs.

# smoelius: Allow `iter-without-into-iter` until the following issue is resolved:
# https://github.com/bitflags/bitflags/issues/379

cargo +nightly clippy --all-features --all-targets "$@" -- \
    -D warnings \
    -W clippy::pedantic \
    -A clippy::missing-errors-doc \
    -A clippy::missing-panics-doc \
    -A clippy::needless_pass_by_ref_mut \
    -A clippy::iter-without-into-iter
