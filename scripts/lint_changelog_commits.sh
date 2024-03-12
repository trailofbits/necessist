#! /bin/bash

# set -x
set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "$0: expect no arguments" >&2
    exit 1
fi

grep -ow '[[:xdigit:]]\{40\}' CHANGELOG.md |
while read X; do
    if git log --pretty=format:'%H' | grep "$X" >/dev/null; then
        continue
    fi
    echo "$X"
    exit 1
done
