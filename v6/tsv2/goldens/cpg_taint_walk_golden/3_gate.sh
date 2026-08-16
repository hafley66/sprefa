#!/usr/bin/env bash
# 3_gate.sh -- THE TAINT WALK, GRADED BYTE-FOR-BYTE.
#
# One program, one corpus, the RUST door (emit_rust_harness --live-hosts). Both
# host families run LINKED: `files` routes to SoopyFilesExecutor and the six
# extraction projections to SprefaExtractExecutor (hosts.rs executor_for_plan),
# so no `git` child and no `extract` child is spawned. DL_EXTRACT_BIN is unset
# below on purpose, so a subprocess spelling fails loudly instead of passing.
#
# ── WHAT A DIFF HERE MEANS ──────────────────────────────────────────────────
# The graded artifact is the settled row set of eight rels, folded from the tick
# log and sorted, against 2_expected.walk.tsv. Byte offsets are in those rows,
# so editing ANY corpus file moves them and this gate goes red. That is the
# intent: the corpus is a pinned input, not a sample.
#
# ── THE THREE SHAPE CLAIMS, EACH OVER ITS OWN POPULATION ────────────────────
# A rel that never arrived and a rel that arrived empty read the same in a diff,
# so every emptiness claim below is paired with the population it was made over.
#   sanitized_handler.rs   MUST NOT taint. Its source reaches the sink through
#                          escape_sql, and the `not(sanitizer_node(Mid))` stop is
#                          the only thing cutting it. Graded against hop_count,
#                          because with zero hops it would be empty for free.
#   unrelated_handler.rs   MUST NOT taint. A source and a sink with no path.
#   two_site_handler.rs    MUST taint under `tainted` and MUST NOT under
#                          `site_tainted`. That difference IS cfl_blocked, and a
#                          gate that only checked `tainted` would never see it.
#
# ── SABOTAGE RECEIPTS (run 2026-08-16, all reverted) ────────────────────────
# 1  SANITIZER STOP REMOVED. Dropped `not(sanitizer_node(...))` from reach_hop:
#      FAIL  SHAPE sanitized_handler.rs taints (1 rows); the sanitizer stop or
#            the corpus changed
#    The byte diff also grew `tainted` and `cfl_blocked` rows for that file. The
#    stop is what holds the sanitized arm, not the shape of the corpus.
# 2  RET HOP DE-INDEXED. Replaced top_step's shared SiteStart with two free
#    variables, so a return may leave through any site:
#      FAIL  PIN two_site_handler.rs: the site-indexed walk taints it too
#            (1 rows); the call-site index is not being read
#    The two walks agreed and the pinned false path vanished. This is the
#    assertion that proves the site index is doing work.
# 3  JOIN KEYED ON THE WHOLE SPAN. Bound df_arg's call end to the call_site span
#    end as well as its start:
#      FAIL  SHAPE hop_count is EMPTY, so every claim resting on it is untested
#      FAIL  SHAPE flow_arg_to_param is EMPTY, so "sanitized_handler.rs does not
#            taint" is untested
#      FAIL  SHAPE tainted found NO path in tainted_handler.rs; the boundary hop
#            is broken
#    7 assertions failed. Zero hops means zero taint, and a rig that graded only
#    "the sanitized arm is empty" would have called that green; the population
#    pairing is what turns it red.

set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$HERE/../.." && pwd)"
V6="$(cd "$TSV2/.." && pwd)"
REPO="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
EXTRACT="$V6/sprefa-extract"
HARNESS="${DL_RUST_HARNESS:-$ENGINE/target/release/emit_rust_harness}"
EXTRACT_BIN="${DL_EXTRACT_BIN:-$EXTRACT/target/release/extract}"
PROGRAM="$HERE/0_cpg_taint_walk.dl6"
SCHEDULE="$HERE/1_schedule.json"
EXPECTED="$HERE/2_expected.walk.tsv"
CORPUS_GLOB="v6/tsv2/goldens/cpg_taint_walk_golden/corpus/*.rs"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/cpg-taint-walk.XXXXXX")"
trap 'rm -rf -- "$WORK"' EXIT
failures=0

say() { printf '%s\n' "$*"; }
note_failure() { printf 'FAIL  %s\n' "$*"; failures=$((failures + 1)); }
stop() { printf 'FAIL  %s\n' "$*"; exit 1; }

GRADED="source_node sink_node sanitizer_node flow_arg_to_param flow_ret_to_call_res tainted site_tainted cfl_blocked"

# ── the corpus must be IN THE INDEX, because `files` is git ls-files ────────
tracked="$(cd "$REPO" && git ls-files -- "$CORPUS_GLOB" | wc -l | tr -d ' ')"
[ "$tracked" = "4" ] \
  || stop "git ls-files reports $tracked corpus files, this rig is written for 4 (git add the corpus)"
say "PASS  corpus: 4 tracked files under $CORPUS_GLOB"

# ── the two binaries. A gate does not build what it can find. ───────────────
if [ ! -x "$EXTRACT_BIN" ]; then
  cargo build --quiet --manifest-path "$EXTRACT/Cargo.toml" --release --features cli --bin extract \
    >"$WORK/extract-build.log" 2>&1 || stop "extractor build: $(tail -5 "$WORK/extract-build.log")"
fi
[ -x "$EXTRACT_BIN" ] || stop "no extractor at $EXTRACT_BIN"
if [ ! -x "$HARNESS" ]; then
  cargo build --quiet --release --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
    >"$WORK/harness-build.log" 2>&1 || stop "harness build: $(tail -5 "$WORK/harness-build.log")"
fi
[ -x "$HARNESS" ] || stop "no harness at $HARNESS"
say "PASS  harness $HARNESS"

# ── the rust door ───────────────────────────────────────────────────────────
swipl -q -l "$V6/prolog/compile.pl" -l "$V6/prolog/emit_rust.pl" \
  -g "compile_dl6('$PROGRAM','$WORK/program.rs',[emitter(emit_rust:emit_program)])" -g halt \
  >"$WORK/compile.log" 2>&1 \
  || stop "0_cpg_taint_walk.dl6 did not compile through emit_rust.pl.
      Reason: $(tail -3 "$WORK/compile.log")
      Bucket it against $V6/prolog/compile/out/manifest.json before calling it a language limit."
[ -s "$WORK/program.rs" ] || stop "emit_rust wrote no program"
say "PASS  compiled through the rust door"

started="$SECONDS"
( cd "$REPO" && env -u DL_EXTRACT_BIN "$HARNESS" "$WORK/program.rs" "$SCHEDULE" --live-hosts ) \
  >"$WORK/ticks.jsonl" 2>"$WORK/run.err" \
  || stop "harness stopped: $(tail -5 "$WORK/run.err")"
[ -s "$WORK/ticks.jsonl" ] || stop "the harness printed no tick log"
say "PASS  ran on the rust door in $((SECONDS - started))s, $(wc -l <"$WORK/ticks.jsonl" | tr -d ' ') ticks"

# ── fold the tick log into one sorted table, rel name in column 1 ───────────
python3 - "$WORK/ticks.jsonl" "$WORK/actual.walk.tsv" "$WORK/counts.tsv" $GRADED <<'PYTHON'
import json
import sys

ticks_path, out_path, counts_path = sys.argv[1], sys.argv[2], sys.argv[3]
graded = set(sys.argv[4:])
counted = {"source_count", "sink_count", "hop_count"}
settled = {}
for line in open(ticks_path):
    line = line.strip()
    if not line.startswith("{"):
        continue
    for rel, delta in json.loads(line)["deltas"].items():
        if rel not in graded and rel not in counted:
            continue
        rows = settled.setdefault(rel, set())
        rows.update(tuple(str(cell) for cell in row) for row in delta["add"])
        rows.difference_update(tuple(str(cell) for cell in row) for row in delta["del"])

with open(out_path, "w") as handle:
    for rel in sorted(graded):
        for row in sorted(settled.get(rel, ())):
            handle.write("\t".join((rel,) + row) + "\n")

with open(counts_path, "w") as handle:
    for rel in sorted(counted | graded):
        handle.write("%s\t%d\n" % (rel, len(settled.get(rel, ()))))
PYTHON
[ -s "$WORK/actual.walk.tsv" ] || stop "the projection produced no rows for any graded rel"

rel_rows() { awk -F'\t' -v want="$1" '$1 == want {n++} END {print n + 0}' "$WORK/counts.tsv"; }
rows_of() { awk -F'\t' -v want="$1" '$1 == want {print $2}' "$WORK/counts.tsv"; }
rows_for_file() {
  awk -F'\t' -v want="$1" -v file="$2" '$1 == want && index($2, file) {n++} END {print n + 0}' \
    "$WORK/actual.walk.tsv"
}

# ── ASSERTION 1: the byte diff, which is the whole golden ───────────────────
if [ ! -f "$EXPECTED" ]; then
  cp "$WORK/actual.walk.tsv" "$EXPECTED"
  stop "no expected file; wrote the current answer to $EXPECTED. Review it, then re-run."
fi
if diff -u "$EXPECTED" "$WORK/actual.walk.tsv" >"$WORK/walk.diff"; then
  say "PASS  walk table byte-identical to 2_expected.walk.tsv ($(wc -l <"$EXPECTED" | tr -d ' ') rows)"
else
  note_failure "the walk table differs from 2_expected.walk.tsv"
  sed 's/^/DIFF    /' "$WORK/walk.diff" | head -40
fi

# ── ASSERTION 2: every population an emptiness claim is made over ───────────
for rel in source_count sink_count hop_count flow_ret_to_call_res; do
  got="$(rows_of "$rel")"
  if [ "${got:-0}" -gt 0 ]; then
    say "SHAPE $rel has $got rows"
  else
    note_failure "SHAPE $rel is EMPTY, so every claim resting on it is untested"
  fi
done

# ── ASSERTION 3: the sanitized and unrelated arms must NOT taint ────────────
hops="$(rows_of flow_arg_to_param)"
for arm in sanitized_handler.rs unrelated_handler.rs; do
  got="$(rows_for_file tainted "$arm")"
  if [ "${hops:-0}" = "0" ]; then
    note_failure "SHAPE flow_arg_to_param is EMPTY, so \"$arm does not taint\" is untested"
  elif [ "$got" = "0" ]; then
    say "SHAPE $arm does not taint, over $hops arg-to-param hops"
  else
    note_failure "SHAPE $arm taints ($got rows); the sanitizer stop or the corpus changed"
  fi
done

# ── ASSERTION 4: the tainted arm MUST taint, on both walks ─────────────────
for walk in tainted site_tainted; do
  got="$(rows_for_file "$walk" tainted_handler.rs)"
  if [ "$got" -gt 0 ]; then
    say "SHAPE $walk reaches the sink in tainted_handler.rs ($got rows)"
  else
    note_failure "SHAPE $walk found NO path in tainted_handler.rs; the boundary hop is broken"
  fi
done

# ── ASSERTION 5: the CFL receipt. The two walks MUST disagree. ─────────────
naive="$(rows_for_file tainted two_site_handler.rs)"
indexed="$(rows_for_file site_tainted two_site_handler.rs)"
blocked="$(rows_of cfl_blocked)"
if [ "$naive" = "0" ]; then
  note_failure "PIN two_site_handler.rs: the naive walk found no path, so the CFL pin tests nothing"
elif [ "$indexed" != "0" ]; then
  note_failure "PIN two_site_handler.rs: the site-indexed walk taints it too ($indexed rows); the call-site index is not being read"
elif [ "${blocked:-0}" = "0" ]; then
  note_failure "PIN cfl_blocked is EMPTY over $hops arg-to-param hops; the two walks agree and the discipline is unexercised"
else
  say "PIN   two_site_handler.rs: naive taints ($naive), site-indexed refuses (0), cfl_blocked $blocked"
fi

[ "$failures" = "0" ] || stop "$failures gate assertion(s) failed"
say "CPG TAINT WALK GRADED: $(wc -l <"$EXPECTED" | tr -d ' ') rows byte-stable over 8 rels, \
$hops arg-to-param hops, $blocked false path(s) removed by the call-site index"
