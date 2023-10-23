#! /bin/bash

# set -x
set -euo pipefail

# smoelius: The next command should match the `clippy` test in necessist/tests/ci.rs.

cargo +nightly clippy --all-features --all-targets "$@" -- \
    -D warnings \
    -W clippy::pedantic \
    -A clippy::missing-errors-doc \
    -A clippy::missing-panics-doc \
    -A clippy::struct-field-names
