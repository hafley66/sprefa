#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/v6/sprefa-engine-rs"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
corpus="$scratch/corpus"
mkdir -p "$corpus"

cargo build --quiet --manifest-path "$here/Cargo.toml" --bin emit_rust_harness

swipl -q -l "$root/v6/prolog/sweep.pl" -l "$here/grade.pl" \
  -g "rust_grade:generate('$corpus','$scratch/compile.tsv')" -g halt
( cd "$root/v6/prolog/conformance" && swipl -q -l ticklog.pl -l "$root/v6/dd-runner/sweep_oracle.pl" \
  -g "sweep_oracle('$corpus','$scratch/oracle.tsv')" -g halt )

text_door_program="$scratch/text-door.rs"
swipl -q -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
  -g "compile_dl6('$root/v6/dl/fixtures/door-handwritten.dl6','$text_door_program',[emitter(emit_rust:emit_program)])" -g halt
check_dir="$scratch/compile-check"
mkdir -p "$check_dir/src"
printf '[package]\nname = "emitted-rust-check"\nversion = "0.0.0"\nedition = "2021"\n\n[dependencies]\nserde_json = "1"\nsprefa-engine-rs = { path = "%s" }\n' "$here" >"$check_dir/Cargo.toml"
printf 'include!("%s");\nfn main() { let _ = program(); }\n' "$text_door_program" >"$check_dir/src/main.rs"
cargo build --quiet --manifest-path "$check_dir/Cargo.toml"

verdicts="$scratch/verdicts.tsv"
: >"$verdicts"
reason_text() {
  tr '\n\r\t' ' ' \
    | sed -E 's/_[0-9]+/_/g; s/, line: [0-9]+, column: [0-9]+//g; s/[[:space:]]+/ /g; s/^ //; s/ $//' \
    | cut -c1-180 \
    | sed 's/ $//'
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
      reason=$({ diff -u "$oracle" "$output" || true; } | awk 'NR > 2 && /^[+-]/ { print; exit }' | reason_text)
    fi
  else
    verdict=runtime-error
    reason=$({ grep -A1 -m1 'panicked at' "$scratch/$name.err" || true; } | sed -n '2p' | reason_text)
    [ -n "$reason" ] || reason=$(head -n1 "$scratch/$name.err" | reason_text)
  fi
  printf '%s\t%s\t%s\n' "$name" "$verdict" "$reason" >>"$verdicts"
done <"$scratch/compile.tsv"

sort "$verdicts" -o "$verdicts"
ratchet="$here/graded.tsv"
clean_now=$(awk -F'\t' '$2=="clean"' "$verdicts" | wc -l | tr -d ' ')
graded_total=$(wc -l <"$verdicts" | tr -d ' ')
status=0
if [ -f "$ratchet" ]; then
  lost=$(comm -23 <(awk -F'\t' '$2=="clean" {print $1}' "$ratchet") <(awk -F'\t' '$2=="clean" {print $1}' "$verdicts"))
  gained=$(comm -13 <(awk -F'\t' '$2=="clean" {print $1}' "$ratchet") <(awk -F'\t' '$2=="clean" {print $1}' "$verdicts"))
  if [ -n "$lost" ]; then printf 'RUST-GRADE REGRESSION\n%s\n' "$lost"; status=1; fi
  if [ -n "$gained" ]; then printf 'RUST-GRADE RATCHET\n%s\n' "$gained"; status=1; fi
else
  printf 'graded.tsv missing\n'
  status=1
fi
[ -n "${RUST_GRADE_WRITE_GRADED:-}" ] && cp -f "$verdicts" "$ratchet"
printf 'RUST-GRADE graded=%s byte-clean=%s\n' "$graded_total" "$clean_now"
for verdict in runtime-error diff unsupported error compiled; do
  count=$(awk -F'\t' -v verdict="$verdict" '$2 == verdict { count++ } END { print count + 0 }' "$verdicts")
  [ "$count" = 0 ] && continue
  printf '  %s %s\n' "$verdict" "$count"
  awk -F'\t' -v verdict="$verdict" '$2 == verdict { print $3 }' "$verdicts" \
    | sort | uniq -c | sort -rn \
    | while read -r cause_count cause; do
        printf '    %s  %s\n' "$cause_count" "$cause"
      done
done
exit "$status"
