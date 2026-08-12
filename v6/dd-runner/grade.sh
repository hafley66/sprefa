#!/usr/bin/env bash
set -euo pipefail

export LC_ALL=en_US.UTF-8
root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/v6/dd-runner"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

arm=${DD_RUNNER_ARM:---dd-diet-rust-sqlite}
corpus="$scratch/corpus"
mkdir -p "$corpus"

# The emitter's own plunit suite is the `plunit` leg's job; running it here
# imported that leg's red into this one and stopped the grade under set -e.
cargo build --quiet --release --manifest-path "$here/Cargo.toml"
runner="$here/target/release/dd-runner"

swipl -q -l "$root/v6/prolog/compile/6_emit_dd_plan.pl" -l "$here/sweep_plans.pl" \
  -g "sweep('$root','$corpus','$scratch/plans.tsv')" -g halt
( cd "$root/v6/prolog/conformance" && swipl -q -l ticklog.pl -l "$here/sweep_oracle.pl" \
  -g "sweep_oracle('$corpus','$scratch/oracle.tsv')" -g halt )

# One ratchet per arm: each arm names its own graded.tsv, so a rename cannot
# leave the sqlite arm silently grading against a kernel-arm expectation.
# arm is a leading-dash flag (--dd-diet-rust-sqlite); strip the dashes for the
# file name (graded.dd-diet-rust-sqlite.tsv).
ratchet="$here/graded.${arm#--}.tsv"

# Peak RSS is the whole point of the ceiling: in TypeScript an unbounded row
# unload OOMed and announced itself, in Rust it grows quietly.
if [ "$(uname -s)" = Darwin ]; then
  peak_rss_kb() { awk '/maximum resident set size/ { print int($1 / 1024) }' "$1"; }
  time_cmd=(/usr/bin/time -l)
else
  peak_rss_kb() { tail -1 "$1"; }
  time_cmd=(/usr/bin/time -f %M)
fi

verdicts="$scratch/verdicts.tsv"
: >"$verdicts"
peak_kb=0
peak_fixture=none
for plan in "$corpus"/*.json; do
  name=$(basename "$plan" .json)
  oracle="$corpus/$name.oracle.jsonl"
  [ -f "$oracle" ] || continue
  out="$scratch/$name.out"
  measured="$scratch/$name.time"
  if "${time_cmd[@]}" "$runner" "$plan" "$arm" >"$out" 2>"$measured"; then
    if diff -q "$oracle" "$out" >/dev/null 2>&1; then verdict=clean; else verdict=diff; fi
  else
    verdict=error
  fi
  rss_kb=$(peak_rss_kb "$measured" 2>/dev/null || echo 0)
  [ -n "$rss_kb" ] || rss_kb=0
  if [ "$rss_kb" -gt "$peak_kb" ]; then peak_kb=$rss_kb; peak_fixture=$name; fi
  printf '%s\t%s\n' "$name" "$verdict" >>"$verdicts"
done

sort "$verdicts" -o "$verdicts"
graded="$ratchet"
clean_now=$(awk -F'\t' '$2=="clean"' "$verdicts" | wc -l | tr -d ' ')
graded_total=$(wc -l <"$verdicts" | tr -d ' ')

status=0
if [ -f "$graded" ]; then
  lost=$(comm -23 <(awk -F'\t' '$2=="clean" {print $1}' "$graded") \
                  <(awk -F'\t' '$2=="clean" {print $1}' "$verdicts"))
  gained=$(comm -13 <(awk -F'\t' '$2=="clean" {print $1}' "$graded") \
                    <(awk -F'\t' '$2=="clean" {print $1}' "$verdicts"))
  if [ -n "$lost" ]; then
    printf 'GRADE REGRESSION, these were byte-clean and are not:\n%s\n' "$lost"
    status=1
  fi
  if [ -n "$gained" ]; then
    printf 'GRADE RATCHET, newly byte-clean; copy the run into %s:\n%s\n' "$(basename "$graded")" "$gained"
    status=1
  fi
else
  printf '%s missing; writing the current run for arm %s is a human decision\n' "$(basename "$graded")" "$arm"
  status=1
fi
[ -n "${DD_RUNNER_WRITE_GRADED:-}" ] && cp "$verdicts" "$graded"

peak_mb=$(( peak_kb / 1024 ))
ceiling=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['conformance_corpus']['peak_rss_mb_ceiling'])" "$here/budget.json")
if [ "$peak_mb" -gt "$ceiling" ]; then
  printf 'RSS CEILING BREACH %s MB > %s MB (worst fixture %s)\n' "$peak_mb" "$ceiling" "$peak_fixture"
  status=1
fi

printf 'DD-GRADE arm=%s graded=%s byte-clean=%s peak_rss_mb=%s (%s kB, %s) ceiling=%s\n' \
  "$arm" "$graded_total" "$clean_now" "$peak_mb" "$peak_kb" "$peak_fixture" "$ceiling"
[ "$status" = 0 ] && printf 'DD-GRADE HOLDS\n'
exit "$status"
