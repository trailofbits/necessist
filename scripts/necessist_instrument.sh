#! /bin/bash

# set -x
set -eo pipefail

if [[ $# -ne 0 ]]; then
    echo "$0: expect no arguments" >&2
    exit 1
fi

if [[ -z "$NECESSIST_PATH" ]]; then
    echo "$0: please set NECESSIST_PATH to the absolute path where necessist resides" >&2
    exit 1
fi

if [[ -n "$(git status --ignored=traditional --porcelain)" ]]; then
    echo "$0: repository contains uncommitted changes; please run in a clean repository" >&2
    exit 1
fi

write_build_script() {
    if [[ -e "$1" ]]; then
        echo "Updating existing build script \`$1\`"
        sed -i 's/\<\(fn main() {\)\(.*\)$/\1\
    println!("cargo:rerun-if-env-changed=NECESSIST_SOURCE_FILE");\
    println!("cargo:rerun-if-env-changed=NECESSIST_START_LINE");\
    println!("cargo:rerun-if-env-changed=NECESSIST_START_COLUMN");\
    println!("cargo:rerun-if-env-changed=NECESSIST_END_LINE");\
    println!("cargo:rerun-if-env-changed=NECESSIST_END_COLUMN");\
\2/' "$1"
    else
        cat << EOF > "$1"
fn main() {
    println!("cargo:rerun-if-env-changed=NECESSIST_SOURCE_FILE");
    println!("cargo:rerun-if-env-changed=NECESSIST_START_LINE");
    println!("cargo:rerun-if-env-changed=NECESSIST_START_COLUMN");
    println!("cargo:rerun-if-env-changed=NECESSIST_END_LINE");
    println!("cargo:rerun-if-env-changed=NECESSIST_END_COLUMN");
}
EOF
    fi
}

update_cargo_toml() {
    if [[ ! -e "$1" ]]; then
        echo "$0: $1 does not exist" >&2
        exit 1
    fi
    if grep -- '^\[dependencies.necessist\]$' "$1" > /dev/null; then
        echo "$0: $1 already depends upon necessist" >&2
        exit 1
    fi
    cat << EOF >> "$1"

[dependencies.necessist]
path = "$NECESSIST_PATH"
EOF
}

update_rust_sources() {
    find "$1" -name '*.rs' |
    while read X; do
        if grep -- '^[[:space:]]*#\[necessist::necessist\]$' "$X" > /dev/null; then
            # echo "$0: $X already uses necessist; skipping" >&2
            continue
        fi
        sed -i 's/^\([[:space:]]*\)#\[test\]/\1#[necessist::necessist]\n&/' "$X"
    done
}

instrument_package() {
    write_build_script "$1/build.rs"
    update_cargo_toml "$1/Cargo.toml"
    update_rust_sources "$1"
}

if grep -- '^\[workspace\]$' Cargo.toml > /dev/null; then
    sed -n '/members = \[/,/]/p' Cargo.toml |
    grep -o '"[^,]*"' |
    sed 's/"\([^,]*\)"/\1/' |
    while read X; do
        if [[ -z "$X" ]]; then
            continue
        fi
        instrument_package "$X"
    done
else
    instrument_package '.'
fi
