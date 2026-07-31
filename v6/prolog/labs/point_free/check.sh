#!/usr/bin/env bash
# check.sh <dir/name> : `bop check` one lab program, printing the exit code.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../../.." && pwd)"
cd "$REPO/v6/tsv2"
out="$(npm run --silent bop -- check "$HERE/$1.dl6" 2>&1)"
code=$?
printf '%s\n' "$out" | grep -v 'ExperimentalWarning\|trace-warnings' || true
echo "check exit: $code"
