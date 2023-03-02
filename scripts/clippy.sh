#! /bin/bash

# set -x
set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "$0: expect no arguments" >&2
    exit 1
fi

# smoelius: The next command should match the `clippy` test in core/tests/ci_is/enabled.rs.

cargo +nightly clippy --all-features --all-targets -- \
    -D warnings \
    -W clippy::pedantic \
    -A clippy::let-underscore-untyped \
    -A clippy::missing-errors-doc \
    -A clippy::missing-panics-doc
