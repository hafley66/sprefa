#!/usr/bin/env bash
# Every fixture the shared guard admits, folded and diffed against grade.sh's oracle.
set -uo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/v6/sprefa-engine-rs"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT INT TERM
corpus="$scratch/corpus"
mkdir -p "$corpus"

cargo build --quiet --manifest-path "$here/Cargo.toml" --bin emit_rust_harness || exit 1

swipl -q -l "$root/v6/prolog/sweep.pl" -l "$here/shared-frontier-grade.pl" \
  -g "shared_frontier_grade:generate('$corpus','$scratch/compile.tsv')" -g halt
( cd "$root/v6/prolog/conformance" \
  && swipl -q -l ticklog.pl -l "$root/v6/dd-runner/sweep_oracle.pl" \
  -g "sweep_oracle('$corpus','$scratch/oracle.tsv')" -g halt )

verdicts="$scratch/verdicts.tsv"
: >"$verdicts"
reason_text() {
  tr '\n\r\t' ' ' \
    | sed -E 's/_[0-9]+/_/g; s/, line: [0-9]+, column: [0-9]+//g; s/[[:space:]]+/ /g; s/^ //; s/ $//' \
    | cut -c1-180 | sed 's/ $//'
}

while IFS=$'\t' read -r name compile_result compile_reason; do
  oracle="$corpus/$name.oracle.jsonl"
  schedule="$corpus/$name.schedule.json"
  program="$corpus/$name.rs"
  output="$scratch/$name.out"
  if [ "$compile_result" != compiled ] || [ ! -f "$oracle" ]; then
    verdict="$compile_result"
    reason=$(printf '%s' "$compile_reason" | reason_text)
    [ "$compile_result" != compiled ] || reason='no oracle tick log'
  elif "$here/target/debug/emit_rust_harness" "$program" "$schedule" >"$output" 2>"$scratch/$name.err"; then
    if diff -q "$oracle" "$output" >/dev/null 2>&1; then
      verdict=clean
      reason='byte-clean'
    else
      verdict=diff
      reason='tick log differs from the oracle'
    fi
  else
    verdict=runtime-error
    reason=$({ grep -A1 -m1 'panicked at' "$scratch/$name.err" || true; } | sed -n '2p' | reason_text)
    [ -n "$reason" ] || reason=$(head -n1 "$scratch/$name.err" | reason_text)
  fi
  printf '%s\t%s\t%s\n' "$name" "$verdict" "$reason" >>"$verdicts"
done <"$scratch/compile.tsv"

sort "$verdicts" -o "$verdicts"
graded_total=$(wc -l <"$verdicts" | tr -d ' ')
clean_now=$(awk -F'\t' '$2=="clean"' "$verdicts" | wc -l | tr -d ' ')
printf 'SHARED-GRADE graded=%s byte-clean=%s\n' "$graded_total" "$clean_now"
for verdict in runtime-error diff unsupported error compiled; do
  count=$(awk -F'\t' -v verdict="$verdict" '$2 == verdict { count++ } END { print count + 0 }' "$verdicts")
  [ "$count" = 0 ] && continue
  printf '  %s %s\n' "$verdict" "$count"
  awk -F'\t' -v verdict="$verdict" '$2 == verdict { print $3 }' "$verdicts" \
    | sort | uniq -c | sort -rn \
    | while read -r cause_count cause; do printf '    %s  %s\n' "$cause_count" "$cause"; done
done

# A shared arm that folds but disagrees with the oracle is the only red here;
# a guard stop is expected and counted, never a failure.
bad=$(awk -F'\t' '$2=="diff" || $2=="runtime-error"' "$verdicts" | wc -l | tr -d ' ')
[ "$bad" = 0 ] || exit 1
