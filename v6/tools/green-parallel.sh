#!/usr/bin/env bash
# @comment-ok: the per-leg parallel-safety constraint below is the file's contract
# green-parallel.sh -- green-all's legs, phased. `just green-all-serial` is
# the one-at-a-time fallback.
#
# A leg may only join the parallel phase if load cannot move its verdict:
#   scale-floor   reads a wall clock. Alone, first, on a quiet machine.
#   memory-soak   compares its final quarter to its second. Alone: a load ramp
#                 lands in the final quarter and reddens rss_flat.
#   serial phase  writes gen_emitted/ or gen/scale_generated.ts; tsv2-test
#                 times a boot; getting-started sleeps 3 for a watcher tick.

set -uo pipefail

V6="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
JOBS="${GREEN_PARALLEL_JOBS:-6}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/green-parallel.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

PHASE_WALL=(scale-floor)

PHASE_A=(conformance roundtrip text-door sweep import-gate
         staleness-gate docs-staleness golden-flex tsv2-test getting-started)

# Each entry is "leg[:ENV=VALUE...]". Ordered longest-first: on a bounded pool
# the makespan is set by the slowest job if it starts last.
SOAK="memory-soak:TSV2_SOAK_PORT=17801"

PHASE_B=(
  "multirepo-golden:TSV2_MULTIREPO_PORT=17812"
  "precommit-changed:TSV2_GIT_DIAGS_PORT=17814"
  "flagship:TSV2_FLAGSHIP_PORT=17808 TSV2_FLAGSHIP_FLOW_PORT=17809"
  "store-test"
  "dd-grade"
  "files:TSV2_FILES_PORT=17807"
  "extraction-live:TSV2_EXTRACTION_PORT=17806"
  "serve-leak-soak:TSV2_LEAK_PORT=17805"
  "prolog-lint"
  "serve-endurance:TSV2_ENDURANCE_PORT=17804"
  "compile-speed"
  "plunit"
  "rust-grade"
  "typecheck"
  "rtkq-golden:TSV2_PARITY_PORT=17813"
  "watch-scale"
  "catalog-audit:CATALOG_AUDIT_PORT=17815"
  "ghcacher-golden:TSV2_PARITY_PORT=17811"
  "typegen-golden:TYPEGEN_PORT=17820"
  "one-subscribe"
)

started="$(date +%s)"
status=0

# Rows go to refs/notes/perf, never the worktree: a dirty tree after every gate
# run is why goldens/scale-floor-history.jsonl is ignored. Keys match that file.
GREEN_RUN="g-$(date +%s | tail -c 7)$$"
GREEN_COMMIT="$(git -C "$V6" rev-parse --short HEAD 2>/dev/null || echo unknown)"
GREEN_PHASE=wall
export GREEN_RUN GREEN_COMMIT GREEN_PHASE

run_leg() {
  local spec="$1" leg env_assignments
  leg="${spec%%:*}"
  env_assignments=""
  [ "$spec" != "$leg" ] && env_assignments="${spec#*:}"
  local begin; begin="$(date +%s)"
  ( cd "$V6" && env $env_assignments just "$leg" ) >"$WORK/$leg.out" 2>&1
  local rc=$?
  local sec=$(( $(date +%s) - begin ))
  printf '%s' "$rc" >"$WORK/$leg.rc"
  printf '%s' "$sec" >"$WORK/$leg.sec"
  # Single write under 4KB: O_APPEND keeps the xargs -P 6 writers from interleaving.
  printf '{"at":"%s","commit":"%s","run":"%s","phase":"%s","leg":"%s","seconds":%s,"rc":%s}\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$GREEN_COMMIT" "$GREEN_RUN" \
    "$GREEN_PHASE" "$leg" "$sec" "$rc" >>"$WORK/history.jsonl"
  return $rc
}
export -f run_leg
export V6 WORK

report() {
  local leg rc sec
  for leg in "$@"; do
    rc="$(cat "$WORK/$leg.rc" 2>/dev/null || echo '?')"
    sec="$(cat "$WORK/$leg.sec" 2>/dev/null || echo '?')"
    if [ "$rc" = "0" ]; then
      printf 'PASS  %-20s %4ss\n' "$leg" "$sec"
    else
      status=1
      printf 'FAIL  %-20s %4ss  exit=%s\n' "$leg" "$sec" "$rc"
      sed 's/^/      | /' <"$WORK/$leg.out" | tail -25
    fi
  done
}

GREEN_PHASE=wall
for leg in "${PHASE_WALL[@]}"; do run_leg "$leg"; done
report "${PHASE_WALL[@]}"

GREEN_PHASE=soak
run_leg "$SOAK"
report memory-soak

echo
echo "== serial legs (${#PHASE_A[@]}) =="
GREEN_PHASE=A
for leg in "${PHASE_A[@]}"; do run_leg "$leg"; done
report "${PHASE_A[@]}"

echo
echo "== parallel legs, ${JOBS} at a time (${#PHASE_B[@]}) =="
GREEN_PHASE=B
printf '%s\n' "${PHASE_B[@]}" | xargs -P "$JOBS" -I{} bash -c 'run_leg "$@"' _ {}
# Unquoted on purpose: report/1 takes one leg name per argument.
# shellcheck disable=SC2046
report $(printf '%s\n' "${PHASE_B[@]}" | sed 's/:.*//')

echo
elapsed=$(( $(date +%s) - started ))
if [ "$status" = "0" ]; then
  echo "GREEN ALL HOLDS: $(( ${#PHASE_WALL[@]} + 1 + ${#PHASE_A[@]} + ${#PHASE_B[@]} )) legs, ${elapsed}s"
else
  echo "GREEN ALL FAILED after ${elapsed}s"
fi

printf '{"at":"%s","commit":"%s","run":"%s","phase":"total","leg":"__gate","seconds":%s,"rc":%s}\n' \
  "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$GREEN_COMMIT" "$GREEN_RUN" "$elapsed" "$status" \
  >>"$WORK/history.jsonl"
# Recording a run must never redden it, and a concurrent worktree can hold the ref lock.
git -C "$V6" notes --ref=perf append -F "$WORK/history.jsonl" 2>/dev/null \
  || git -C "$V6" notes --ref=perf append -F "$WORK/history.jsonl" 2>/dev/null \
  || echo "perf note not recorded (run $GREEN_RUN)"

exit "$status"
