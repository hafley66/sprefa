#!/usr/bin/env bash
# two_door.sh <program.dl6> <schedule.json> [settle] [--except <rel>]
#
# Runs one program through BOTH doors and diffs the per-rel delta sequences.
# Exit 0 = identical.
#
# `--except <rel>` drops ONE rel from both sides and says so in the output. It
# exists for exactly one case in this lab and is never used silently: the
# GraphQL introspection document echoes itself back through the world-fed
# `introspection` rel, and that echo is the only place json-flex card C3 (the
# oracle's json null is the ATOM `none`, the emitter's is a real null) can bite.
# Every DERIVED fact still has to match. Using it anywhere else would be hiding
# a divergence, so the receipt prints the exclusion on every run.

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib.sh"

program="$1"
schedule="$2"
settle="${3:-0.35}"
except=""
if [ "${4:-}" = "--except" ]; then
  except="${5:-}"
fi

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

filter() {
  if [ -n "$except" ]; then
    grep -v "^${except} " || true
  else
    cat
  fi
}

oracle_log "$program" "$schedule" | sequence | filter >"$scratch/oracle"
served_log "$program" "$schedule" "$settle" | sequence | filter >"$scratch/served"

if [ ! -s "$scratch/oracle" ]; then
  echo "ORACLE PRODUCED NOTHING for $program"
  exit 1
fi

note=""
[ -n "$except" ] && note=" [EXCLUDED rel '$except', see header]"

if diff -u "$scratch/oracle" "$scratch/served" >"$scratch/diff"; then
  printf 'IDENTICAL %s (%s deltas, both doors)%s\n' \
    "$(basename "$program")" "$(wc -l <"$scratch/oracle" | tr -d ' ')" "$note"
  exit 0
fi

printf 'DIVERGED %s%s\n' "$(basename "$program")" "$note"
sed -n 1,40p "$scratch/diff"
exit 1
