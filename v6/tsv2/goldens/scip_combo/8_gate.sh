#!/usr/bin/env bash
# 8_gate.sh -- SEVEN AUTHORED PROGRAMS, TWO DOORS, ONE CORPUS, JUDGED BY DIFF.
#
# The rig is 2_gate.sh's, with the graded pair changed. There the two sides were
# two ENGINES (v5 binary against v6). Here they are two DOORS of v6 running the
# SAME program text over the SAME corpus in the same run:
#
#   RUST DOOR   emit_rust.pl -> emit_rust_harness --live-hosts. The ruled host
#               names execute IN-PROCESS: SoopyFilesExecutor for the four file
#               feeds, SprefaExtractExecutor for the extractor, DEP_CRAWL for
#               the four crawl names (hosts.rs executor_for_plan).
#   TS DOOR     the served tsv2 engine. No linked arms exist; every host runs
#               its emitted shell template.
#
# So a rel that differs is a disagreement between a linked twin and the template
# it claims to be equivalent to, and nothing else in the tree compares them.
#
# ─── THE EXPECTED ANSWER IS NOT "EVERYTHING AGREES" ─────────────────────────
# Two programs are PINNED DISAGREEMENTS. Their doors answer DIFFERENTLY by
# design, and the difference is what each pin grades:
#
#   7_door_skew_family     `--family diet_scip` is a whole-project MODE the CLI
#                          implements and the in-process mask parser refuses by
#                          name, so the Rust door stops with a named error. The
#                          gate grades the STOP: a silent rc=0 with zero rows is
#                          the regression, and it is what this pin turns RED on.
#
#   2_extract_rev_skew     the Rust door's SprefaExtractExecutor reads extract
#                          bytes by blob oid, so the HEAD-pinned feed answers a
#                          DIFFERENT callable set than the dirty worktree feed;
#                          the TS door's emitted template reads the worktree for
#                          both. The doors differ on the dirty-worktree arm, and
#                          that difference is the point of the pin.
#
# 6_door_skew_files_at WAS a third pin (the rev-parse phantom row). Its template
# now carries the verified guard and both doors agree, so it is graded
# MUST-AGREE; this gate goes RED if the phantom row returns.
#
# ─── ONE PROGRAM HAS ONE DOOR, AND ITS OWN TEXT PROVES IT ───────────────────
# 4_crawl_extract's two `dep_crawl_*` templates end in `exit 3`. On the TS door
# they are what actually runs, so the program cannot load there. That is not a
# skip: assertion 0 pins the `exit 3` templates, so "the crawl is linked only on
# the Rust door" is a fact this rig demonstrates rather than a claim it repeats.
#
# ─── SABOTAGE RECEIPTS (run 2026-08-16, all reverted) ───────────────────────
# 1  HALF THE FIX APPLIED TO THE PIN. Changed 6_door_skew_files_at's template to
#    `git rev-parse --verify --quiet`, which is the guard half of the README's
#    fix and NOT the whole fix:
#      DOOR 6_door_skew_files_at.pinned_file: DIFFERS (rust 2 rows, ts 0 rows)
#    The doors still disagree, the other way round. `--verify --quiet` prints
#    nothing for an absent path, so `[ -n "$oid" ] && printf` leaves the while
#    loop's exit status at 1 whenever the LAST enumerated path is the absent
#    one, and a nonzero exit is a host failure that answers zero rows. That is
#    crawl_org.dl6:68-70's `if`-not-`&&` measurement meeting this template, and
#    it is why the README prescribes BOTH halves. Receipt that a plausible
#    one-line repair is not one.
# 2  FAMILY NAME CHANGED. Changed 7_door_skew_family's `--family diet_scip` to
#    `--family call`, a mask name both doors read:
#      FAIL  PIN 7_door_skew_family: BOTH DOORS AGREE -- the pinned defect is gone
#      FAIL  1 gate assertion(s) failed
#    Exactly the receipt that this gate grades the DEFECT and notices its repair.
# 3  CORPUS ARM REMOVED. Dropped the uncommitted append to beta/src/use.ts from
#    0_corpus.sh, so no path's worktree bytes differ from HEAD:
#      FAIL  SHAPE 2_extract_rev_skew: pinned_digest_skew has 0 rows, the corpus
#            is built for 1
#    The two anti-joins stayed empty and green over 5 rows instead of 6, which
#    is the point: without that one dirty file they are empty for a reason that
#    proves nothing, and only the population check catches it.
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"
ENGINE="$REPO/v6/sprefa-engine-rs"
EXTRACT="$REPO/v6/sprefa-extract"
HARNESS="${DL_RUST_HARNESS:-$ENGINE/target/debug/emit_rust_harness}"
EXTRACT_BIN="${DL_EXTRACT_BIN:-$EXTRACT/target/release/extract}"
SERVE_MAIN="$TSV2/serve/main.ts"
PORT="${TSV2_SCIP_COMBO_PORT:-17607}"
BASE="http://127.0.0.1:$PORT"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-scip-combo.XXXXXX")"
CORPUS="$WORK/corpus"
MANIFEST="$REPO/v6/prolog/compile/out/manifest.json"
SERVER_PID=""
failures=0

say() { printf '%s\n' "$*"; }
note_failure() { printf 'FAIL  %s\n' "$*"; failures=$((failures + 1)); }
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT
stop() { printf 'FAIL  %s\n' "$*"; stop_server; exit 1; }

PROGRAMS="1_rev_file_skew 2_extract_rev_skew 3_span_in_file 4_crawl_extract 5_cross_repo_ref 6_door_skew_files_at 7_door_skew_family"

# Every rel this rig grades, per program. The list is the program's `?` queries
# minus the ones whose rows carry a scratch path that is not a corpus root.
graded_rels() {
  case "$1" in
    1_rev_file_skew) echo "base_file head_file added_path removed_path rewritten_path added_count removed_count" ;;
    2_extract_rev_skew) echo "worktree_file pinned_file pinned_digest_skew worktree_proc pinned_proc worktree_only_proc pinned_only_proc agreed_proc_count skewed_file_count" ;;
    3_span_in_file) echo "file_extent proc_span contained_span span_outside_file orphan_span contained_count extent_count" ;;
    4_crawl_extract) echo "crawl_visit visited_root rootless_visit visited_file repo_family_count barren_repo reached_repo_count" ;;
    5_cross_repo_ref) echo "proc_def call_site cross_repo_ref local_ref dangling_ref cross_repo_count dangling_count" ;;
    6_door_skew_files_at) echo "pinned_file pinned_file_count" ;;
    7_door_skew_family) echo "scanned_file resolved_edge resolved_edge_count" ;;
  esac
}

both_doors() { [ "$1" != "4_crawl_extract" ]; }

# TWO CLASSES, and every program is in exactly one.
#   pinned      the program exists to hold a door disagreement
#   agreeing    everything else, and a difference there is unexplained
must_differ() { case "$1" in 7_door_skew_family|2_extract_rev_skew) return 0 ;; *) return 1 ;; esac; }

# program 7 pins a named STOP on the Rust door: the extract twin refuses the
# mode name. Grading the stop means rc nonzero AND stderr naming the family.
# rc=0 with zero rows is the silent regression this pin turns RED on.
rust_should_stop() { [ "$1" = "7_door_skew_family" ]; }

# ── the corpus, and the pins every leg reads ────────────────────────────────
bash "$HERE/0_corpus.sh" "$CORPUS" >"$WORK/corpus.log" 2>&1 \
  || stop "corpus build failed: $(cat "$WORK/corpus.log")"
[ -f "$CORPUS/REVS.tsv" ] || stop "0_corpus.sh wrote no REVS.tsv"
corpus_repos="$(wc -l <"$CORPUS/REVS.tsv" | tr -d ' ')"
[ "$corpus_repos" = "4" ] || stop "the corpus has $corpus_repos repositories, this rig is written for 4"
say "PASS  corpus: 4 repositories at $CORPUS, two revisions each"

# ── assertion 0: the linked-arm receipt the crawl program carries ───────────
crawl_exits="$(grep -c "is linked in-process' >&2; exit 3" "$HERE/4_crawl_extract.dl6")"
[ "$crawl_exits" = "2" ] \
  || stop "4_crawl_extract.dl6 has $crawl_exits of 2 templates ending in exit 3; the one-door receipt is gone"
say "PASS  both dep_crawl_* templates exit 3, so a TS-door fall-through cannot answer rows"

# ── the two binaries. A gate does not build what it can find. ───────────────
if [ ! -x "$HARNESS" ]; then
  cargo build --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
    >"$WORK/harness-build.log" 2>&1 || stop "harness build failed: $(tail -5 "$WORK/harness-build.log")"
fi
[ -x "$HARNESS" ] || stop "no harness at $HARNESS"
if [ ! -x "$EXTRACT_BIN" ]; then
  cargo build --quiet --manifest-path "$EXTRACT/Cargo.toml" --features cli --bin extract \
    >"$WORK/extract-build.log" 2>&1 || stop "extractor build failed: $(tail -5 "$WORK/extract-build.log")"
fi
[ -x "$EXTRACT_BIN" ] || stop "no extractor at $EXTRACT_BIN"
export DL_EXTRACT_BIN="$EXTRACT_BIN"
say "PASS  harness $HARNESS, extractor $EXTRACT_BIN"

# ── the arrivals, one schedule per program, derived from REVS.tsv ───────────
python3 - "$CORPUS" "$WORK" <<'PY' || stop "schedule generation failed"
import json, pathlib, sys
corpus, work = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
pins = [line.rstrip("\n").split("\t") for line in open(corpus / "REVS.tsv")]
seeds = [f"example.com/{slug}" for slug, _a, _b, _root in pins]
alpha = next(row for row in pins if row[0] == "alpha")
batches = {
    "1_rev_file_skew": [["want_rev_pair", [root, rev_a, rev_b, "*"]] for _s, rev_a, rev_b, root in pins],
    "2_extract_rev_skew": [["want_repo_pair", [root, rev_b, "*.ts"]] for _s, _a, rev_b, root in pins],
    "3_span_in_file": [["want_scan", [root, "*.ts"]] for _s, _a, _b, root in pins],
    "4_crawl_extract": [["want_crawl", [str(corpus), seed, "go_mod"]] for seed in seeds],
    "5_cross_repo_ref": [["want_scan", [root, "*.ts"]] for _s, _a, _b, root in pins],
    "6_door_skew_files_at": [["want_rev", [alpha[3], alpha[1], "*"]]],
    "7_door_skew_family": [["want_scan", [root, "*.ts"]] for _s, _a, _b, root in pins],
}
for program, rows in batches.items():
    (work / f"{program}.schedule.json").write_text(json.dumps(
        [[{"rel": rel, "sign": "add", "row": row} for rel, row in rows]]))
    (work / f"{program}.arrivals.json").write_text(json.dumps(
        {"batch": [{"rel": rel, "sign": "add", "row": row} for rel, row in rows]}))
PY

# ── the Rust door ───────────────────────────────────────────────────────────
for program in $PROGRAMS; do
  generated="$WORK/$program.rs"
  swipl -q -l "$REPO/v6/prolog/compile.pl" -l "$REPO/v6/prolog/emit_rust.pl" \
    -g "compile_dl6('$HERE/$program.dl6','$generated',[emitter(emit_rust:emit_program)])" -g halt \
    >"$WORK/$program.compile.log" 2>&1 \
    || stop "$program.dl6 did not compile through emit_rust.pl.
      Reason: $(tail -3 "$WORK/$program.compile.log")
      Bucket it against $MANIFEST before calling it a language limit."
  [ -s "$generated" ] || stop "emit_rust wrote no program for $program"

  started="$(date +%s)"
  if rust_should_stop "$program"; then
    "$HARNESS" "$generated" "$WORK/$program.schedule.json" --live-hosts \
      >"$WORK/$program.ticks.jsonl" 2>"$WORK/$program.harness.err"
    rc=$?
    if [ "$rc" = "0" ]; then
      note_failure "PIN $program: the Rust door answered rc=0 with zero rows; the silent-zero-rows defect is back"
    elif grep -q "diet_scip" "$WORK/$program.harness.err"; then
      say "PIN   $program: the Rust door named the unknown family and stopped (rc=$rc)"
    else
      note_failure "PIN $program: the Rust door stopped (rc=$rc) but did not name diet_scip: $(tail -2 "$WORK/$program.harness.err")"
    fi
    continue
  fi
  "$HARNESS" "$generated" "$WORK/$program.schedule.json" --live-hosts \
    >"$WORK/$program.ticks.jsonl" 2>"$WORK/$program.harness.err" \
    || stop "$program stopped on the Rust door: $(tail -3 "$WORK/$program.harness.err")"
  [ -s "$WORK/$program.ticks.jsonl" ] || stop "$program printed no tick log on the Rust door"

  python3 - "$WORK" "$program" <<'PY' || stop "$program tick-log projection failed"
import json, pathlib, sys
work, program = pathlib.Path(sys.argv[1]), sys.argv[2]
rows = {}
for line in open(work / f"{program}.ticks.jsonl"):
    line = line.strip()
    if not line.startswith("{"):
        continue
    for rel, delta in json.loads(line)["deltas"].items():
        if rel.startswith("__"):
            continue
        settled = rows.setdefault(rel, set())
        settled.update(tuple(str(cell) for cell in row) for row in delta["add"])
        settled.difference_update(tuple(str(cell) for cell in row) for row in delta["del"])
for rel, settled in rows.items():
    (work / f"{program}.rust.{rel}.tsv").write_text(
        "".join("\t".join(row) + "\n" for row in sorted(settled)))
PY
  say "PASS  $program on the Rust door in $(( $(date +%s) - started ))s"
done

# ── the TS door ─────────────────────────────────────────────────────────────
TSV2_DB=":memory:" TSV2_PORT="$PORT" DL_EXTRACT_BIN="$EXTRACT_BIN" \
  node --experimental-transform-types "$SERVE_MAIN" >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 80); do
  curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || stop "server died on boot: $(tail -5 "$WORK/server.log")"
  sleep 0.25
done

for program in $PROGRAMS; do
  both_doors "$program" || continue
  status="$(curl -s -o "$WORK/$program.load.json" -w '%{http_code}' \
    -X POST --data-binary @"$HERE/$program.dl6" "$BASE/program")"
  [ "$status" = "200" ] || stop "$program load returned $status on the TS door: $(cat "$WORK/$program.load.json")"
  curl -s -o /dev/null -X POST --data-binary @"$WORK/$program.arrivals.json" "$BASE/edb/events"

  # Settle = four consecutive identical count vectors over the graded rels.
  probe=""
  for rel in $(graded_rels "$program"); do probe="$probe $rel"; done
  last=""; stable=0; settled=""; deadline=$((SECONDS + 180))
  while [ "$SECONDS" -lt "$deadline" ]; do
    now=""
    for rel in $probe; do
      count="$(curl -s "$BASE/idb/$rel" | python3 -c 'import json,sys
try: print(len(json.load(sys.stdin)["rows"]))
except Exception: print(-1)')"
      now="$now:$count"
    done
    if [ "$now" = "$last" ]; then
      stable=$((stable + 1))
      [ "$stable" -ge 3 ] && { settled="$now"; break; }
    else
      stable=0
    fi
    last="$now"
    sleep 1
  done
  [ -n "$settled" ] || stop "$program never settled on the TS door (last counts $last)"

  for rel in $probe; do
    curl -s "$BASE/idb/$rel" | python3 -c '
import json, sys
rows = json.load(sys.stdin)["rows"]
sys.stdout.write("".join("\t".join(str(cell) for cell in row) + "\n" for row in sorted(rows)))' \
      | LC_ALL=C sort -u >"$WORK/$program.ts.$rel.tsv"
  done
  say "PASS  $program on the TS door, graded counts$settled"
done
stop_server

# ── THE DOOR DIFF ───────────────────────────────────────────────────────────
agreeing=0; differing=0
for program in $PROGRAMS; do
  both_doors "$program" || { say "DOOR  $program: RUST ONLY, its dep_crawl_* templates exit 3 on the TS door"; continue; }
  program_differs=0
  for rel in $(graded_rels "$program"); do
    rust="$WORK/$program.rust.$rel.tsv"; ts="$WORK/$program.ts.$rel.tsv"
    [ -f "$rust" ] || : >"$rust"
    LC_ALL=C sort -u "$rust" -o "$rust"
    [ -f "$ts" ] || : >"$ts"
    if cmp -s "$rust" "$ts"; then
      agreeing=$((agreeing + 1))
    else
      differing=$((differing + 1)); program_differs=$((program_differs + 1))
      say "DOOR  $program.$rel: DIFFERS (rust $(wc -l <"$rust" | tr -d ' ') rows, ts $(wc -l <"$ts" | tr -d ' ') rows)"
      diff "$rust" "$ts" | sed 's/^/DOOR    /' | head -8
    fi
  done
  if must_differ "$program"; then
    if [ "$program_differs" = "0" ]; then
      note_failure "PIN $program: BOTH DOORS AGREE -- the pinned defect is gone, rewrite or retire the pin"
    else
      say "PIN   $program: doors disagree on $program_differs rel(s), which is what this file exists to hold"
    fi
  elif [ "$program_differs" != "0" ]; then
    note_failure "$program: $program_differs rel(s) differ across the doors and nothing in the program says why"
  fi
done

# ── THE SHAPE ASSERTIONS ────────────────────────────────────────────────────
# A rel that never arrived and a rel that arrived empty read the same in a diff,
# so every emptiness claim is paired with the population it was made over.
rust_rows() { [ -f "$WORK/$1.rust.$2.tsv" ] && wc -l <"$WORK/$1.rust.$2.tsv" | tr -d ' ' || printf '0\n'; }

check_empty() {
  local program="$1" rel="$2" population="$3" why="$4"
  local empty population_rows
  empty="$(rust_rows "$program" "$rel")"
  population_rows="$(rust_rows "$program" "$population")"
  if [ "$population_rows" = "0" ]; then
    note_failure "SHAPE $program: $population is EMPTY, so \"$rel is empty\" is untested"
  elif [ "$empty" != "0" ]; then
    note_failure "SHAPE $program: $rel has $empty rows; $why"
    head -5 "$WORK/$program.rust.$rel.tsv" | sed 's/^/SHAPE   /'
  else
    say "SHAPE $program: $rel empty over $population_rows rows of $population ($why)"
  fi
}

check_nonempty() {
  local program="$1" rel="$2" want="$3"
  local got
  got="$(rust_rows "$program" "$rel")"
  if [ "$got" = "$want" ]; then
    say "SHAPE $program: $rel has $got rows"
  else
    note_failure "SHAPE $program: $rel has $got rows, the corpus is built for $want"
  fi
}

check_names() {
  local program="$1" rel="$2" column="$3" want="$4"
  local got
  got="$(cut -f"$column" "$WORK/$program.rust.$rel.tsv" 2>/dev/null | sed 's|.*/||' | LC_ALL=C sort -u | paste -sd, -)"
  if [ "$got" = "$want" ]; then
    say "SHAPE $program: $rel column $column is exactly {$want}"
  else
    note_failure "SHAPE $program: $rel column $column is {$got}, the corpus is built for {$want}"
  fi
}

check_nonempty 1_rev_file_skew added_path 4
check_nonempty 1_rev_file_skew removed_path 1
check_nonempty 1_rev_file_skew rewritten_path 1
check_names 1_rev_file_skew removed_path 2 "main.go"

check_nonempty 2_extract_rev_skew pinned_digest_skew 1
check_empty 2_extract_rev_skew pinned_only_proc agreed_proc "the pinned feed must not answer a callable that exists only in the dirty worktree"
check_nonempty 2_extract_rev_skew worktree_only_proc 1
check_names 2_extract_rev_skew worktree_only_proc 3 "beta_uncommitted"

check_empty 3_span_in_file span_outside_file contained_span "a def span past its own file's byte count is an extractor defect"
check_empty 3_span_in_file orphan_span contained_span "a span whose file the feed never reported means the two disagree about the corpus"

check_empty 4_crawl_extract rootless_visit visited_root "a visited coordinate with no checkout row would drop out of the extraction unseen"
check_nonempty 4_crawl_extract barren_repo 4
check_names 4_crawl_extract barren_repo 2 "shared"

check_names 5_cross_repo_ref cross_repo_ref 4 "alpha"
check_nonempty 5_cross_repo_ref cross_repo_ref 2
check_nonempty 5_cross_repo_ref local_ref 1
check_names 5_cross_repo_ref dangling_ref 3 "ghost_call,trim"

[ "$failures" = "0" ] || { say "artifacts: $WORK"; stop "$failures gate assertion(s) failed"; }
say "artifacts: $WORK"
say "SCIP COMBO GRADED: 7 programs compiled, $agreeing rels byte-identical across the doors, \
$differing differing under 2 pinned defects, \
1 program Rust-door-only by its own templates, 0 unexplained"
