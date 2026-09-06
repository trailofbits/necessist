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

VERSION_GO="$(
    cat versions.json |
    jq -r '.[] | select(.name == "GO") | .version'
)"

cd "$WORKSPACE"/necessist/tests/third_party_tests

URL='https://github.com/golang/go'

# smoelius: Use the approach of update_testing_tool_versions.sh in this directory.
VERSION_TAG="$(
    git ls-remote --tags --refs "$URL" |
    cut -f2 |
    sed 's,^refs/tags/go,,' |
    grep '^[0-9]\+\.[0-9]\+\.[0-9]\+$' |
    sort -V |
    tail -n 1
)"

# smoelius: Require that the Go version used for the third party tests is not newer than the Go
# version installed in CI.
VERSION_NEW="$(echo -e "$VERSION_GO\n$VERSION_TAG" | sort -V | head -n 1)"

find . -name '*.toml' |
while read X; do
    REV="$(cat "$X" | sed -n 's/^rev = "\([^"]*\)"$/\1/;T;p')"
    if [[ ! "$REV" =~ ^go.* ]]; then
        continue;
    fi
    sed -i "s/^rev = \"[^\"]*\"$/rev = \"go$VERSION_NEW\"/" "$X"
done
