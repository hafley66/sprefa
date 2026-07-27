#!/usr/bin/env bash
# e2e dish path: prolog compiles rules to SQL, piped sqlite3 executes.
dir="$(cd "$(dirname "$0")/.." && pwd)"
exec /opt/homebrew/bin/swipl -q -s "$dir/swi_sqlite_reach.pl" -- "$1" "$2"
