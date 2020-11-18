#! /bin/bash

COMMIT=37dba3abe9aa0a1cc6b9762353bf6dd89b9ef6a5

# set -x
set -eu

pushd .. && source env.sh && popd

rm -rf tmp
mkdir tmp
cd tmp

git clone https://github.com/AltSysrq/proptest.git
cd proptest
git checkout "$COMMIT"

necessist_instrument.sh

# smoelius: This is an expensive test. To speed things up, undo some of `necessist_instrument.sh`'s
# changes.
find . -name '*.rs' |
while read X; do
    if [[ "$X" =~ .*/build.rs ]]; then
        continue
    fi
    if ! grep '^[[:space:]]*proptest!' "$X" > /dev/null; then
        echo -n "Undoing changes to $X: "
        git checkout "$X" 2>&1
    fi
done

cargo necessist --sqlite --skip-calls '.*' --skip-controls --skip-locals

readarray -t STMTS < <(sqlite3 necessist.db 'select stmt from removal')
readarray -t URLS < <(sqlite3 necessist.db 'select url from removal')

git checkout .

I=0
while [[ $I -lt ${#STMTS[@]} ]]; do
    STMT="$(echo "${STMTS[$I]}" | tr -d '[[:space:]]')"
    URL="${URLS[$I]}"
    FILE="$(expr "$URL" : ".*/$COMMIT/\(.*\)#L[0-9]\+-L[0-9]\+$")"
    START="$(expr "$URL" : ".*/$COMMIT/.*#L\([0-9]\+\)-L[0-9]\+$")"
    END="$(expr "$URL" : ".*/$COMMIT/.*#L[0-9]\+-L\([0-9]\+\)$")"
    cat "$FILE" |
        tail -n +"$START" |
        head -n "$((1 + $END - $START))" |
        sed 's,//.*,,' |
        tr -d '[[:space:]]' |
        fgrep "$STMT"
    I=$((1+$I))
done

exit 0
