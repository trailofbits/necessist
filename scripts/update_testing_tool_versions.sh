#! /bin/bash

# set -x
set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "$0: expect no arguments" >&2
    exit 1
fi

SCRIPTS="$(dirname "$(realpath "$0")")"
WORKSPACE="$(realpath "$SCRIPTS"/..)"

cd "$WORKSPACE"/.github/actions/install-testing-tools

cat versions.json |
jq -c '.[]' |
while read X; do
    NAME="$(echo "$X" | jq -r '.name')"
    URL="$(echo "$X" | jq -r '.url')"
    VERSION_OLD="$(echo "$X" | jq -r '.version')"

    TAG="$(
        git ls-remote --tags --refs "$URL" |
        cut -f2 |
        sed 's,^refs/tags/\(v\|go\),,' |
        grep '^[0-9]\+\.[0-9]\+\.[0-9]\+$' |
        sort -V |
        tail -n 1
    )"

    VERSION_NEW="$(echo -e "$VERSION_OLD\n$TAG" | sort -V | tail -n 1)"

    TMP="$(mktemp)"
    cat versions.json | jq "map(if .name == \"$NAME\" then .version = \"$VERSION_NEW\" end)" > "$TMP"
    mv "$TMP" versions.json
done
