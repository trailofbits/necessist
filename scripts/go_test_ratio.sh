#! /bin/bash

# smoelius: This script outputs the number of tests executed per second for each Go test file under
# the current directory. The purpose is to estimate the "bang for your buck" of each Go test file.

rm -rf ~/.cache/go-build

grep -l -r -w defer --include=*_test.go |
xargs -n 1 dirname |
sort -u |
while read X; do
    Y="$(go test ./"$X" | grep -m 1 '^ok\>')"
    if [[ -z "$Y" || "$Y" =~ cached ]]; then
        continue
    fi
    N="$(grep -I '^func Test' "$X"/*_test.go | wc -l)"
    MILLIS="$(echo "$Y" | sed 's/^.*[[:space:]]\([0-9]\+\)\.\([0-9]\{3\}\)s$/\1\2/;s/^0*//')"
    echo "$X" $(($N*1000/$MILLIS))
done |
tee /dev/tty |
sort -k 2 -n
