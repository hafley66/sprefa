#!/usr/bin/env bash
# oracle_pairs.sh : the ORACLE leg of the composition grade. The accumulation
# class (`pre`-fed folds) is refused by the COMPILER for both spellings alike
# (edge_body_needs_pre, the pre_occurrence_loop arc), so those pairs are graded
# where they run: the reference engine, through the .dl6 text door, tick log
# diffed byte for byte.
#
# The text door matters here rather than the term door: a term-door fixture is
# one prolog term whose rules SHARE variable cells, so any grading that
# substitutes into a rule leaks into its siblings. parse_dl gives each rule its
# own binding environment, which is the real per-rule scoping.

set -uo pipefail
LAB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$LAB/out"

identical=0
differing=0

grade_pair() {
  local label="$1" bind="$2" expr="$3" schedule="$4"
  bash "$LAB/oracle.sh" "$bind" "$schedule" > "$LAB/out/$label.bind.log" 2>&1
  bash "$LAB/oracle.sh" "$expr" "$schedule" > "$LAB/out/$label.expr.log" 2>&1
  if diff -q "$LAB/out/$label.bind.log" "$LAB/out/$label.expr.log" >/dev/null; then
    echo "TICKLOG_IDENTICAL  $label"
    identical=$((identical + 1))
  else
    echo "DIFFERING          $label"
    diff "$LAB/out/$label.bind.log" "$LAB/out/$label.expr.log" | head -6
    differing=$((differing + 1))
  fi
}

grade_pair counter_fold        S1_counter_bind        S1_counter_head        S1_counter.schedule.json
grade_pair concat_fold         S2_concat_fold_bind    S2_concat_fold_head    S2_concat.schedule.json
grade_pair state_machine       S3_statemachine_bind   S3_statemachine_head   S3.schedule.json
grade_pair queue_two_heads     S4_queue_bind          S4_queue_head          S4.schedule.json
grade_pair edge_head_self_carry A1_edge_head_bind     A1_edge_head_expr      A1.schedule.json

echo
echo "RESULT ticklog_identical=$identical differing=$differing"
