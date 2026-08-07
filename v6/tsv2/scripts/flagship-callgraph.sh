#!/usr/bin/env bash
# flagship-callgraph.sh — THE V5-OUTPUT-VS-V6 GRADING RIG (golden plan phase 3,
# ARCH fork flagship_pick: "callgraph rail first proves the rig, flow-interproc
# rides the same rig").
#
# It diffs, rel by rel, over ONE pinned corpus:
#   v5: CHECKED-IN OUTPUT of the root `dl` binary running
#       `examples/callgraph-ast.dl` BYTE-UNMODIFIED, pinned under
#       goldens/flagship_callgraph/v5_golden/ with the rev, the binary digest and
#       the corpus digest it was captured against
#   v6: the served tsv2 engine running `../dl/fixtures/flagship-callgraph.dl6`
#
# THE DEFAULT PATH NEVER RUNS THE V5 BINARY. `FLAGSHIP_V5_WRITE=1` is the one
# mode that does, and it rewrites the golden. Without it a missing golden is a
# hard failure naming that command; nothing is built and nothing regenerates.
# Both pins the golden carries -- the corpus digest and the v5 program's blob
# sha -- are asserted before anything is graded, because a golden diffed against
# a moved corpus is a false green.
#
# ─── WHY THE V5 PROGRAM IS NOT EDITED, AND HOW THE CORPUS IS PINNED ──────────
# `examples/callgraph-ast.dl` hardcodes `scan("WORK", "src/**/*.rs", ...)`. Rather
# than copy it and narrow the glob (which would make "we ran the real rail" a
# claim instead of a fact), the RIG MOVES THE ROOT: it copies a literal, listed
# set of this repository's own Rust files into a scratch directory as `src/...`,
# `git init`s and commits them, and runs with that directory as cwd. The v5
# program is read straight out of examples/ with zero edits, and both engines see
# the same relative paths, so the artifacts compare column for column with no
# path rewriting anywhere in this script.
#
# The corpus is the sprefa-extract crate's core (types / wire / scip / the small
# re-export shims / one language front end / the CLI). It is listed literally
# below rather than globbed out of the repo, so "the pinned corpus" is a fact in
# this file and not a function of what the tree happens to contain. The four large
# per-language front ends (ts / rust / go / kotlin) are deliberately out: they are
# 8k lines whose def x site product is 120k pairs, and the point of the pin is a
# grade that runs in seconds.
#
# ─── ISOLATION ──────────────────────────────────────────────────────────────
# Under `FLAGSHIP_V5_WRITE=1`, `DL_STATE_DIR` points at the scratch tree and
# `--db` names a scratch file, so the capture touches no served-root cache and no
# daemon state. The v6 leg runs on `:memory:`. Nothing under
# ~/.local/state/sprefa is read or written.
#
# ─── THE COLUMN MAPPING (part of the rig, per the brief) ────────────────────
# The two engines name and shape these rels differently. Every artifact is
# LC_ALL=C-sorted, tab-separated, and deduplicated, so a byte diff is the grade.
#
#   artifact   columns        v5 source                    v6 source
#   ────────   ─────────      ──────────────────────────   ─────────────────────
#   file       path           rel_file_txt.path            file(path, _)
#   def        path, name     rel_def_txt (name,path,line) def(path, name, _)
#   call       path, callee   rel_call_txt (callee,path,l) call(path, callee)
#   calls      caller,callee  rel_calls_txt                calls(caller, callee)
#   unused     name           rel_unused_txt               unused(name)
#
# Two columns are DROPPED on each side and both drops are forced, not chosen:
#   - v5's `line` has no v6 counterpart. `sprefa-extract` emits byte spans as a
#     NESTED JSON object and the sh-host decode is a projection over TOP-LEVEL
#     keys only, so a span cannot enter the program as int columns. `decode/2` is
#     the construct that would open it and it is refused (ARCH row decode_arc).
#   - v6's `kind` has no v5 counterpart: v5's tree-sitter query captures a name
#     and nothing else. It is kept in the rel and used by the CLASSIFIER below.
# `reaches` (v5's transitive closure) is not ported and not graded; see the
# program header.
#
# ─── FILE-SET EQUIVALENCE, WHICH IS NOT FREE ────────────────────────────────
# v5's `scan` glob is globset (`*` does NOT cross `/`), so the corpus is spelled
# `src/**/*.rs`. `git ls-files` takes a PATHSPEC (`*` DOES cross `/`, and `**`
# requires at least one directory component), so the SAME file set is spelled
# `src/*.rs`. Measured, not assumed: in this corpus `git ls-files -- 'src/**/*.rs'`
# returns 3 of the 13 files. Assertion 1 below pins that the two spellings select
# the same set before anything is graded.
#
# ─── THE GRADE, AND THE HONESTY RULE ────────────────────────────────────────
# A non-empty diff with every row classified is an HONEST result and exits 0. The
# corpus is never shrunk to empty a diff. Every difference lands in exactly one
# bucket, and `flagship-classify.py` PROVES the bucket from the source bytes
# rather than asserting it:
#
#   (a) extraction-input difference — the two extractors saw different facts.
#       The EXTRACTED rels are proven per row from the corpus bytes (a v6-only
#       `def` is a body-less declaration; a v6-only `call` has no bare-identifier
#       occurrence). The DERIVED rels are proven differently and more strongly:
#       v5's two rule bodies are written once in the classifier and run against
#       EACH engine's own def/call sets, and each engine must return its own
#       answer. Both holding means the two engines compute the same function, so
#       every remaining row is a function of the inputs.
#   (b) v6 expression gap — the port could not say what v5 says.
#   (c) real defect — neither of the above.
#
# Any row the classifier cannot place in (a) is reported and the script exits 1:
# an unexplained difference is a failure of this rig, not a footnote.
#
# ─── SABOTAGE RECEIPTS (run 2026-07-29, both reverted) ──────────────────────
# 1  `def(path, name, kind) <- node_fact(path, 'site', kind, name).` — the wrong
#    record tag. `def` / `calls` / `unused` all go empty and the rig FAILS with
#    57 unclassified rows, every one a v5-only `def`: the row-shape proofs only
#    ever explain v6-ONLY rows, so a v5 row v6 does not have is unexplainable by
#    construction. Note which leg catches it — rule fidelity does NOT, and
#    correctly so: with `def` empty, v5's rules derive empty `calls` and `unused`
#    and v6 returns empty, so the derived rels agree. The extracted rels are
#    where an emptied program has to be caught, and they are.
# 2  dropping `def(_, callee, _)` from the `calls` rule (v5's "the callee must
#    itself be a def" filter) takes `calls` from 574 to 2,851 rows. THIS ONE
#    PASSED an earlier draft of the classifier, which explained a v6-only row by
#    "v5's inputs do not derive it" — true, and irrelevant, because the rule had
#    changed. The rule-fidelity check is what the sabotage bought: it now fails
#    with 2,277 v6-extra rows named individually.
# 3  GOLDEN EDIT, one row, run against the pinned v5 side. In v5.def.tsv,
#    `src/bin/extract.rs family_mode` -> `family_mod`. The row COUNT is
#    unchanged, so the MANIFEST rows_def check passes by design and the grade is
#    what has to catch it. Exit 1, 43 unclassified, the edited name named:
#      ('def', 'src/bin/extract.rs', 'family_mode')
#      ('def', 'v5-only', 'src/bin/extract.rs', 'family_mod')
#      40 calls + 1 unused rows, v5-side rule fidelity broken on that name
#    The receipt is that the checked-in golden FEEDS the grade rather than
#    sitting next to it.
set -uo pipefail

TSV2="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"
PORT="${TSV2_FLAGSHIP_PORT:-17573}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-flagship.XXXXXX")"
ROOT="$WORK/root"
BASE="http://127.0.0.1:$PORT"
V5_PROGRAM="$REPO/examples/callgraph-ast.dl"
V6_PROGRAM="$TSV2/../dl/fixtures/flagship-callgraph.dl6"
SERVE_MAIN="$TSV2/serve/main.ts"
CLASSIFY="$TSV2/scripts/flagship-classify.py"
GOLDEN="$TSV2/goldens/flagship_callgraph/v5_golden"
MANIFEST="$GOLDEN/MANIFEST.tsv"
REGEN_CMD="FLAGSHIP_V5_WRITE=1 bash scripts/flagship-callgraph.sh"
WRITE_GOLDEN="${FLAGSHIP_V5_WRITE:-0}"
SERVER_PID=""

# THE PIN. Paths are relative to v6/sprefa-extract/ and land at the same relative
# path under the scratch root, so both engines say `src/types.rs`.
CORPUS="
src/types.rs
src/wire.rs
src/scip.rs
src/dispatch.rs
src/lib.rs
src/family.rs
src/rows.rs
src/seams.rs
src/shape.rs
src/source.rs
src/lang/mod.rs
src/lang/astgrep.rs
src/bin/extract.rs
"

# The served engine prints one whole tick's deltas per line and this program's
# ticks carry hundreds of rows, so the failure dump is byte-bounded rather than
# line-bounded: `tail -20` here is a 100KB wall of JSON.
fail() { printf 'FAIL  %s\n' "$*"; [ -n "$SERVER_PID" ] && tail -c 2000 "$WORK/server.log"; stop_server; exit 1; }
say() { printf '%s\n' "$*"; }
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT

# ── the two binaries, resolved here and never named in program text ──────────
resolve_v5_bin() {
  if [ -n "${DL_V5_BIN:-}" ] && [ -x "$DL_V5_BIN" ]; then say "v5 bin: $DL_V5_BIN (DL_V5_BIN)"; return; fi
  local release="$REPO/target/release/dl"
  if [ ! -x "$release" ]; then
    fail "no v5 dl binary at $release. A gate does not build; run:
      cd $REPO && cargo build --release --bin dl"
  fi
  [ -x "$release" ] || fail "no v5 dl binary at $release"
  DL_V5_BIN="$release"
  say "v5 bin: $DL_V5_BIN (in-tree release)"
}

resolve_extract_bin() {
  if [ -n "${DL_EXTRACT_BIN:-}" ] && [ -x "$DL_EXTRACT_BIN" ]; then
    say "extract bin: $DL_EXTRACT_BIN (DL_EXTRACT_BIN)"; return
  fi
  local crate release
  crate="$REPO/v6/sprefa-extract"
  release="$crate/target/release/extract"
  if [ ! -x "$release" ]; then
    fail "no release extractor. A gate does not build; run:
      cd $crate && cargo build --release --features cli --bin extract"
  fi
  [ -x "$release" ] || fail "no extract binary at $release"
  export DL_EXTRACT_BIN="$release"
  say "extract bin: $DL_EXTRACT_BIN (in-tree release)"
}

# ── the pinned corpus, as its own one-commit git repository ─────────────────
build_corpus() {
  local count=0 file
  for file in $CORPUS; do
    [ -f "$REPO/v6/sprefa-extract/$file" ] || fail "pinned corpus file missing: v6/sprefa-extract/$file"
    mkdir -p "$ROOT/$(dirname "$file")"
    cp "$REPO/v6/sprefa-extract/$file" "$ROOT/$file"
    count=$((count + 1))
  done
  (cd "$ROOT" && git init -q . && git add -A \
     && git -c user.email=rig@sprefa -c user.name=flagship-rig commit -q -m "pinned corpus") \
    || fail "could not build the pinned corpus repository"
  REV="$(cd "$ROOT" && git rev-parse HEAD)"
  CORPUS_N="$count"
  say "PASS  pinned corpus: $CORPUS_N files at $REV ($ROOT)"
}

# The corpus is COPIED out of the live tree, so it drifts whenever those files
# are edited. Path and bytes both enter the digest, so a rename moves it too.
corpus_digest() {
  local file
  for file in $CORPUS; do
    printf '%s\n' "$file"
    cat "$ROOT/$file"
  done | shasum -a 256 | cut -d' ' -f1
}

manifest_field() { awk -F'\t' -v key="$1" '$1 == key { print $2 }' "$MANIFEST"; }

write_golden() {
  resolve_v5_bin
  git -C "$REPO" diff --quiet -- "$V5_PROGRAM" \
    || say "NOTE  examples/callgraph-ast.dl has uncommitted changes in the source tree"
  DL_STATE_DIR="$WORK/state" "$DL_V5_BIN" "$V5_PROGRAM" --db "$WORK/v5.sqlite" \
    >"$WORK/v5.out" 2>"$WORK/v5.err" || fail "v5 run failed: $(tail -10 "$WORK/v5.err")"
  # The isolation assertion, not a comment about isolation: `DL_STATE_DIR` is the
  # honored sandbox knob, and v5 writes its invocation db and log under whatever
  # that names. Those files existing HERE is the receipt that the real state home
  # was not the one it opened.
  [ -f "$WORK/state/invocations.db" ] \
    || fail "v5 did not write its state under DL_STATE_DIR=$WORK/state; the run was not isolated"
  local v5_files
  v5_files="$(sqlite3 "$WORK/v5.sqlite" 'select count(*) from rel_file_txt;')"
  [ "$v5_files" = "$CORPUS_N" ] \
    || fail "v5 saw $v5_files corpus files, the pin has $CORPUS_N"

  mkdir -p "$GOLDEN"
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" \
    'select distinct path, name from rel_def_txt;'    | LC_ALL=C sort -u >"$GOLDEN/v5.def.tsv"
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" \
    'select distinct path, callee from rel_call_txt;' | LC_ALL=C sort -u >"$GOLDEN/v5.call.tsv"
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" \
    'select distinct caller, callee from rel_calls_txt;' | LC_ALL=C sort -u >"$GOLDEN/v5.calls.tsv"
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" \
    'select distinct name from rel_unused_txt;'       | LC_ALL=C sort -u >"$GOLDEN/v5.unused.tsv"

  {
    printf 'v5_rev\t%s\n'           "$(git -C "$REPO" rev-parse HEAD)"
    printf 'v5_program\t%s\n'       "examples/callgraph-ast.dl"
    printf 'v5_program_blob\t%s\n'  "$(git -C "$REPO" hash-object "$V5_PROGRAM")"
    printf 'v5_binary_sha256\t%s\n' "$(shasum -a 256 "$DL_V5_BIN" | cut -d' ' -f1)"
    printf 'corpus_files\t%s\n'     "$CORPUS_N"
    printf 'corpus_sha256\t%s\n'    "$(corpus_digest)"
    printf 'captured_utc\t%s\n'     "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'regenerate\t%s\n'       "$REGEN_CMD"
    for rel in def call calls unused; do
      printf 'rows_%s\t%s\n' "$rel" "$(wc -l <"$GOLDEN/v5.$rel.tsv" | tr -d ' ')"
    done
  } >"$MANIFEST"
  say "WROTE v5 golden: $GOLDEN (rev $(manifest_field v5_rev), corpus $(manifest_field corpus_sha256))"
}

read_golden() {
  [ -f "$MANIFEST" ] || fail "no pinned v5 golden at $MANIFEST
      A gate does not run v5. Regenerate from v6/tsv2 with:
      $REGEN_CMD"
  local rel want got
  for rel in def call calls unused; do
    [ -f "$GOLDEN/v5.$rel.tsv" ] || fail "pinned v5 golden missing $GOLDEN/v5.$rel.tsv; regenerate: $REGEN_CMD"
    want="$(manifest_field "rows_$rel")"
    got="$(wc -l <"$GOLDEN/v5.$rel.tsv" | tr -d ' ')"
    [ "$want" = "$got" ] \
      || fail "pinned v5 golden v5.$rel.tsv has $got rows, MANIFEST rows_$rel says $want (truncated golden); regenerate: $REGEN_CMD"
    cp "$GOLDEN/v5.$rel.tsv" "$WORK/v5.$rel.tsv"
  done

  want="$(manifest_field corpus_files)"
  [ "$want" = "$CORPUS_N" ] \
    || fail "the pin lists $CORPUS_N files, the golden was captured against $want; regenerate: $REGEN_CMD"
  want="$(manifest_field corpus_sha256)"
  got="$(corpus_digest)"
  [ "$want" = "$got" ] \
    || fail "the corpus MOVED since the v5 golden was captured (golden $want, now $got).
      Grading v6 against a golden from a different corpus is a false green.
      Regenerate from v6/tsv2 with:
      $REGEN_CMD"
  # v5 is retired: the program going away leaves the golden standing, the
  # program CHANGING invalidates it.
  if [ -f "$V5_PROGRAM" ]; then
    want="$(manifest_field v5_program_blob)"
    got="$(git -C "$REPO" hash-object "$V5_PROGRAM")"
    [ "$want" = "$got" ] \
      || fail "$(manifest_field v5_program) changed since the golden was captured (golden blob $want, now $got); regenerate: $REGEN_CMD"
  else
    say "NOTE  $(manifest_field v5_program) is gone from the tree; the golden stands on its recorded rev"
  fi
  say "PASS  v5 golden: rev $(manifest_field v5_rev), captured $(manifest_field captured_utc), corpus digest holds ($GOLDEN)"
}

rows_of() { curl -s "$BASE/idb/$1"; }
row_count() {
  rows_of "$1" | python3 -c 'import json,sys
try: print(len(json.load(sys.stdin)["rows"]))
except Exception: print(-1)'
}

resolve_extract_bin
build_corpus

# ── assertion 1: the two glob spellings select the same file set ────────────
cd "$ROOT"
git ls-files -- 'src/*.rs' | LC_ALL=C sort >"$WORK/fileset.pathspec"
printf '%s\n' $CORPUS | LC_ALL=C sort >"$WORK/fileset.pin"
diff -u "$WORK/fileset.pin" "$WORK/fileset.pathspec" >"$WORK/fileset.diff" \
  || fail "git pathspec 'src/*.rs' does not select the pinned corpus: $(cat "$WORK/fileset.diff")"
narrow="$(git ls-files -- 'src/**/*.rs' | wc -l | tr -d ' ')"
say "PASS  file set: pathspec 'src/*.rs' = the $CORPUS_N pinned files (git's own 'src/**/*.rs' would select only $narrow -- the divergence this rig states)"

# ── the v5 leg: the pinned capture, checked in ──────────────────────────────
[ "$WRITE_GOLDEN" = "1" ] && write_golden
read_golden

# ── the v6 leg: the served tsv2 engine, cwd = the same root ────────────────
TSV2_DB=":memory:" TSV2_PORT="$PORT" DL_EXTRACT_BIN="$DL_EXTRACT_BIN" \
  node --experimental-transform-types "$SERVE_MAIN" >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 60); do
  curl -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || fail "server died on boot: $(tail -5 "$WORK/server.log")"
  sleep 0.2
done

status="$(curl -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$V6_PROGRAM" "$BASE/program")"
[ "$status" = "200" ] || fail "v6 program load returned $status: $(cat "$WORK/load.json")"
say "PASS  v6 program loaded, hosts: $(sed 's/.*"hosts":\[//; s/\].*//' "$WORK/load.json")"

curl -s -o /dev/null -X POST --data-binary \
  "{\"batch\":[{\"rel\":\"want_at\",\"sign\":\"add\",\"row\":[\"$REV\",\"src/*.rs\"]}]}" "$BASE/edb/events"

# Settle = every corpus file present AND four consecutive identical count
# vectors. Counting once would read a mid-extraction snapshot as the answer.
settled=""; last=""; stable=0; deadline=$((SECONDS + 300))
while [ "$SECONDS" -lt "$deadline" ]; do
  now="$(row_count file):$(row_count def):$(row_count call):$(row_count calls):$(row_count unused)"
  if [ "${now%%:*}" = "$CORPUS_N" ] && [ "$now" = "$last" ]; then
    stable=$((stable + 1))
    [ "$stable" -ge 3 ] && { settled="$now"; break; }
  else
    stable=0
  fi
  last="$now"
  sleep 1
done
[ -n "$settled" ] || fail "v6 never settled (last counts file:def:call:calls:unused = $last)"
say "PASS  v6 settled, file:def:call:calls:unused = $settled"

dump() { # dump REL COL_INDEXES OUT
  rows_of "$1" | python3 -c '
import json,sys
cols=[int(c) for c in sys.argv[1].split(",")]
rows=json.load(sys.stdin)["rows"]
out={"\t".join(str(r[c]) for c in cols) for r in rows}
sys.stdout.write("".join(line+"\n" for line in sorted(out)))' "$2" | LC_ALL=C sort -u >"$3"
}
# column orders come from the .dl6 decls:
#   file(path, digest) / def(path, name, kind) / call(path, callee)
#   calls(caller, callee) / unused(name)
dump def   0,1 "$WORK/v6.def.tsv"
dump call  0,1 "$WORK/v6.call.tsv"
dump calls 0,1 "$WORK/v6.calls.tsv"
dump unused 0  "$WORK/v6.unused.tsv"

# ── the grade ───────────────────────────────────────────────────────────────
identical=0; differing=0
for rel in def call calls unused; do
  if cmp -s "$WORK/v5.$rel.tsv" "$WORK/v6.$rel.tsv"; then
    say "GRADE $rel: BYTE-IDENTICAL ($(wc -l <"$WORK/v5.$rel.tsv" | tr -d ' ') rows)"
    identical=$((identical + 1))
  else
    say "GRADE $rel: differs (v5 $(wc -l <"$WORK/v5.$rel.tsv" | tr -d ' ') rows, v6 $(wc -l <"$WORK/v6.$rel.tsv" | tr -d ' ') rows)"
    differing=$((differing + 1))
  fi
done

python3 "$CLASSIFY" "$WORK" "$ROOT" "$DL_EXTRACT_BIN" | tee "$WORK/classification.txt"
grade=${PIPESTATUS[0]}
[ "$grade" = "0" ] || fail "unclassified differences remain (see $WORK/classification.txt)"

stop_server
say "artifacts: $WORK"
say "FLAGSHIP CALLGRAPH GRADED: $identical/4 rels byte-identical, $differing classified, 0 unclassified"
