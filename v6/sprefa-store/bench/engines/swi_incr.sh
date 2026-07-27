#!/usr/bin/env bash
# SWI incremental-tabling contender; swi_reach.pl self-reports the full CSV.
dir="$(cd "$(dirname "$0")/.." && pwd)"
exec /opt/homebrew/bin/swipl -q -s "$dir/swi_reach.pl" -- "$1" "$2"
