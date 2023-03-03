#! /bin/bash

# set -x
set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "$0: expect no arguments" >&2
    exit 1
fi

SCRIPTS="$(dirname "$(realpath "$0")")"
WORKSPACE="$(realpath "$SCRIPTS"/..)"

cd "$WORKSPACE"

package_name() {
    grep -o '^name = "[^"]*"$' Cargo.toml |
    sed 's/^name = "\([^"]*\)"$/\1/'
}

package_version() {
    grep -o '^version = "[^"]*"$' Cargo.toml |
    sed 's/^version = "\([^"]*\)"$/\1/'
}

# smoelius: For an explanation of how/why `published` works the way it does, see:
# https://github.com/trailofbits/dylint/blob/da67ee7450794cb2d6f7efc3202134ffd05465c9/scripts/publish.sh#L26-L44
# See also:
# - https://github.com/rust-lang/crates.io/issues/3512
# - https://github.com/rust-lang/crates.io/discussions/4317
published() {
    pushd "$(mktemp --tmpdir -d tmp-XXXXXXXXXX)"
    trap popd RETURN
    cargo init
    sed -i "/^\[dependencies\]$/a $1 = \"$2\"" Cargo.toml
    echo '[workspace]' >> Cargo.toml
    echo "Checking whether \`$1:$2\` is published ..." >&2
    RUSTFLAGS='-A non_snake_case' cargo check
}

# smoelius: Publishing in this order ensures that all dependencies are met.
DIRS="core frameworks crates_io"

for DIR in $DIRS; do
    pushd "$DIR"

    NAME="$(package_name)"
    VERSION="$(package_version)"

    if published "$NAME" "$VERSION"; then
        popd
        continue
    fi

    cargo publish

    # smoelius: The following should no longer be necessary, given:
    # https://github.com/rust-lang/cargo/pull/11062

    # while ! published "$NAME" "$VERSION"; do
    #     sleep 10s
    # done

    popd
done
