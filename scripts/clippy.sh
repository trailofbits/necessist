#! /bin/bash

# set -x
set -euo pipefail

# smoelius: The next command should match the `clippy` test in core/tests/ci_is/enabled.rs.

cargo +nightly clippy --all-features --all-targets "$@" -- \
    -D warnings \
    -W clippy::pedantic \
    -A clippy::let-underscore-untyped \
    -A clippy::missing-errors-doc \
    -A clippy::missing-panics-doc
