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

find . -name '*.toml' |
while read X; do
    URL="$(cat "$X" | sed -n 's/^url = "\([^"]*\)"$/\1/;T;p')"
    REV="$(cat "$X" | sed -n 's/^rev = "\([^"]*\)"$/\1/;T;p')"
    if [[ -z "$REV" ]]; then
        continue;
    fi
    # smoelius: Skip revisions that are hashes.
    if [[ "$REV" =~ [0-9A-Fa-f]{7,40} ]]; then
        continue;
    fi
    ORG="$(echo "$URL"  | sed -n 's,^https://github.com/\([^/]*\)/[^/]*.*$,\1,;T;p')"
    REPO="$(echo "$URL" | sed -n 's,^https://github.com/[^/]*/\([^/]*\).*$,\1,;T;p')"
    LATEST_RELEASE_URL="https://api.github.com/repos/$ORG/$REPO/releases/latest"
    LATEST_RELEASE="$(curl \
        -H "Accept: application/vnd.github+json" \
        -H "Authentication: Bearer $GITHUB_TOKEN" \
        --silent --show-error "$LATEST_RELEASE_URL")"
    TAG="$(echo "$LATEST_RELEASE" | jq -r .tag_name)"
    if [[ "$TAG" = 'null' ]]; then
        echo -n "$X: "
        echo "$LATEST_RELEASE" | jq .
        continue
    fi
    sed -i "s/^rev = \"[^\"]*\"$/rev = \"$TAG\"/" "$X"
done
