#! /bin/bash

# set -x
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "$0: expect one argument: version" >&2
    exit 1
fi

VERSION="version = \"$1\""

SCRIPTS="$(dirname "$(realpath "$0")")"
WORKSPACE="$(realpath "$SCRIPTS"/..)"

cd "$WORKSPACE"

scripts/lint_changelog_commits.sh

find . -name Cargo.toml ! -path './fixtures/*' -exec sed -i "{
s/^version = \"[^\"]*\"$/$VERSION/
}" {} \;

REQ="${VERSION/\"/\"=}"

find . -name Cargo.toml -exec sed -i "/^necessist/{
s/^\(.*\)\<version = \"[^\"]*\"\(.*\)$/\1$REQ\2/
}" {} \;

# smoelius: The `necessist-audit` skill version must match the package version. See the
# `skill_version_is_necessist_version` test in core/src/skill.rs.
SKILL_VERSION="  version: \"$1\""

sed -i "{
s/^  version: \"[^\"]*\"$/$SKILL_VERSION/
}" core/skills/necessist-audit/SKILL.md

scripts/update_lockfiles.sh
