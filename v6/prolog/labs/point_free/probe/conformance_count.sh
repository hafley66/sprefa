#!/usr/bin/env bash
# Scratch: the conformance corpus count, so this lab can state the untouched
# baseline rather than assert it.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../../.." && pwd)"
cd "$REPO/v6/prolog/conformance"
out="$(swipl -q -l go.pl -g go -g halt 2>&1)"
code=$?
printf 'PASS lines: %s\n' "$(printf '%s\n' "$out" | grep -c '^PASS')"
printf 'FAIL lines: %s\n' "$(printf '%s\n' "$out" | grep -c '^FAIL')"
printf 'exit: %s\n' "$code"
