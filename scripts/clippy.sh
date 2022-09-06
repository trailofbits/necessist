#! /bin/bash

# set -x
set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "$0: expect no arguments" >&2
    exit 1
fi

# cargo clean

cargo clippy --all-targets -- \
    -D warnings \
    -W clippy::pedantic \
    -A clippy::missing-errors-doc \
    -A clippy::missing-panics-doc
