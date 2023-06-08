#! /bin/bash

# set -x
set -euo pipefail

GOLANG="$(find . -name golang | head -n 1)"

SRC=''

cat "$GOLANG"/visitor.rs |
grep -o '\<\(fn \|self\.\)[A-Za-z_]\+(' |
while read X; do
    DECL="$(echo "$X" | sed -n 's/^fn \([A-Za-z_]\+\)($/\1/;T;p')"
    CALL="$(echo "$X" | sed -n 's/^self\.\([A-Za-z_]\+\)($/\1/;T;p')"
    if [[ -n "$DECL" ]]; then
        SRC="$DECL"
    elif [[ -n "$CALL" ]]; then
        echo "    $SRC -> $CALL"
    else
        echo "Failed to parse: $X" >&2
        exit 1
    fi
done |
cat <(echo 'digraph {') - <(echo '}')
