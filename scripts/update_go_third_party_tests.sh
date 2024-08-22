#! /bin/bash

# set -x
set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "$0: expect no arguments" >&2
    exit 1
fi

SCRIPTS="$(dirname "$(realpath "$0")")"
WORKSPACE="$(realpath "$SCRIPTS"/..)"

cd "$WORKSPACE"/necessist/tests/third_party_tests

pushd /tmp
git clone https://github.com/golang/go
cd go
TAG="$(git tag --list --sort=creatordate | tail -n 1)"
popd

rm -rf /tmp/go

find . -name '*.toml' |
while read X; do
    REV="$(cat "$X" | sed -n 's/^rev = "\([^"]*\)"$/\1/;T;p')"
    if [[ ! "$REV" =~ ^go.* ]]; then
        continue;
    fi
    sed -i "s/^rev = \"[^\"]*\"$/rev = \"$TAG\"/" "$X"
done
