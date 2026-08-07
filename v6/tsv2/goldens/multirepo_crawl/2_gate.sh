#!/usr/bin/env bash
# 2_gate.sh -- THE MULTIREPO DEP/REPO/REV GOLDEN (stopping-point program #5).
#
# Same rig as flagship-callgraph.sh, widened from one repository to four:
#   v5: CHECKED-IN OUTPUT of the root `dl` binary running
#       `examples/version-skew.dl` BYTE-UNMODIFIED, pinned under ./v5_golden/
#       with the rev, the binary digest and the corpus digest it was captured
#       against
#   v6: the served tsv2 engine running ./0_multirepo_crawl.dl6
# over ONE pinned corpus, diffed rel by rel, every differing row bucketed with
# a proven reason.
#
# THE DEFAULT PATH NEVER RUNS THE V5 BINARY. `MULTIREPO_V5_WRITE=1` is the one
# mode that does, and it rewrites the golden. Without it a missing golden is a
# hard failure naming that command; nothing is built and nothing regenerates.
# The corpus digest and the v5 program's blob sha are asserted before anything
# is graded: a golden diffed against a moved corpus is a false green.
#
# ─── WHY THE V5 PROGRAM IS NOT EDITED ───────────────────────────────────────
# `examples/version-skew.dl` reads its repo set from $SPREFA_CONFIG. The rig
# therefore BUILDS a config rather than narrowing the program: 1_corpus.sh
# writes four one-commit git repositories and an all.config.toml naming them,
# and the capture is pointed at that file. The program text is read straight out
# of examples/ with zero edits, so "we ran the real rail" is a fact.
#
# The corpus digest covers every corpus file EXCEPT `.git/**` and
# all.config.toml: those two carry a fresh commit sha and the scratch path, so
# they move on every run while the go.mod bytes the program actually reads do
# not. The golden's own columns are slug-keyed for the same reason.
#
# ─── ISOLATION ──────────────────────────────────────────────────────────────
# Under `MULTIREPO_V5_WRITE=1`, `DL_STATE_DIR` points into the scratch tree and
# `--db` names a scratch file, so the capture touches no served-root cache and no
# daemon state; the assertion below proves it by requiring the invocation db to
# exist THERE. The v6 leg runs on `:memory:`. Nothing under
# ~/.local/state/sprefa is read or written.
#
# ─── THE COLUMN MAPPING ─────────────────────────────────────────────────────
#   artifact     columns              v5 source           v6 source
#   ──────────   ──────────────────   ─────────────────   ─────────────────────
#   dep_pin      repo, module, ver    rel_dep_pin_txt     dep_pin(repo,mod,ver)
#   skewed       module               rel_skew_txt col 1  skewed(module)
#   skew_row     module, repo, ver    rel_skew_row_txt    skew_row(...)
#   skew_width   module, repos        rel_skew_width_txt  skew_width(...)
#
# ─── THE ONE NAMED GAP, REPORTED NOT HIDDEN ─────────────────────────────────
# v5's `dep_ver(m, min(v), max(v))` and the lo/hi COLUMNS of its `skew` have no
# v6 counterpart: v6 refuses min/max over text by name,
# `aggregate_operand_not_number(min, _, text)`. So `skewed` grades v5's skew
# MODULE column only, and the gap is printed as a gap. The skew test itself
# never needed the ordering -- v5's own header calls lo/hi "two witnesses, not
# oldest/newest" -- so membership, skew_row and skew_width all grade in full.
#
# A REAL V5 ODDITY the rig surfaced and does not paper over: v5's own
# `rel_dep_ver_txt` returns lo=v0.9.1 hi=v0.8.0 for github.com/pkg/errors, i.e.
# min/max REVERSED against the lexicographic order its header claims, while
# example.com/shared comes back correctly ordered. Since the v6 side refuses
# the construct entirely there is nothing to diff, but it is written down here
# because it is the kind of thing a grading rig exists to notice.
#
# ─── THE GRADE, AND THE HONESTY RULE ────────────────────────────────────────
# Every difference lands in exactly one bucket, proven from the corpus bytes by
# 3_classify.py rather than asserted:
#   (a) extraction-input difference   the two engines saw different facts
#   (b) v6 expression gap             the port could not say what v5 says
#   (c) real defect                   neither of the above
# Any row the classifier cannot place is reported and the script exits 1.
#
# ─── SABOTAGE RECEIPTS (run 2026-07-30, both reverted) ──────────────────────
# 1  CORPUS EDIT, both legs move together. Changed gamma's
#    `github.com/pkg/errors v0.8.0` to `v0.9.1`, so that module stops being
#    skewed. Both engines read the same corpus, so both answers move:
#      settled dep_pin:skewed:skew_row:skew_width  8:2:7:2 -> 8:1:3:1
#      GRADE still 4/4 BYTE-IDENTICAL, CLASSIFY unclassified=0, exit 0
#    That is the correct outcome and it is the receipt that this rig grades
#    AGREEMENT between the engines rather than a table of pinned constants --
#    the numbers are free to move as long as the two legs move together.
# 2  RULE EDIT, one leg only. Changed the v6 skew rule's `left != right` to
#    `left = right`, which makes every module "skewed" including the negative
#    control golang.org/x/sync. The rig went RED and named the control row by
#    row in all three derived rels:
#      GRADE skewed:     differs (v5 2 rows, v6 3 rows)
#      GRADE skew_row:   differs (v5 7 rows, v6 8 rows)
#      GRADE skew_width: differs (v5 2 rows, v6 3 rows)
#      v6 skewed FIDELITY BROKEN     -> golang.org/x/sync
#      v6 skew_row FIDELITY BROKEN   -> golang.org/x/sync  gamma  v0.7.0
#      v6 skew_width FIDELITY BROKEN -> golang.org/x/sync  1
#      CLASSIFY unclassified=3, exit 1
#    Note WHICH leg caught it: the rule-fidelity check, not the row diff. The
#    diff alone says "v6 has an extra row"; fidelity says "v6's own dep_pin does
#    not derive v6's own answer", which is the statement that rules out an
#    input difference. It is also the receipt that the unskewed module in the
#    corpus is a working negative control and not decoration.
# 3  GOLDEN EDIT, one row, run against the pinned v5 side. In v5.dep_pin.tsv,
#    gamma's `github.com/pkg/errors v0.8.0` -> `v0.9.1`. The row COUNT is
#    unchanged, so the MANIFEST rows_dep_pin check passes by design and the grade
#    is what has to catch it. Exit 1, CLASSIFY unclassified=7:
#      GRADE dep_pin: differs (v5 8 rows, v6 8 rows)
#      (c) UNCLASSIFIED, not derivable from corpus bytes
#            gamma  github.com/pkg/errors  v0.9.1
#      v5 skewed / skew_row / skew_width: FIDELITY BROKEN (1 + 4 + 1 rows)
#    The v6 side stays FIDELITY OK, which is what places the edit on the v5 leg.
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$TSV2/../.." && pwd)"
PORT="${TSV2_MULTIREPO_PORT:-17591}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-multirepo.XXXXXX")"
CORPUS="$WORK/corpus"
BASE="http://127.0.0.1:$PORT"
V5_PROGRAM="$REPO/examples/version-skew.dl"
V6_PROGRAM="$HERE/0_multirepo_crawl.dl6"
SERVE_MAIN="$TSV2/serve/main.ts"
CLASSIFY="$HERE/3_classify.py"
GOLDEN="$HERE/v5_golden"
MANIFEST="$GOLDEN/MANIFEST.tsv"
REGEN_CMD="MULTIREPO_V5_WRITE=1 bash goldens/multirepo_crawl/2_gate.sh"
WRITE_GOLDEN="${MULTIREPO_V5_WRITE:-0}"
V5_RELS="dep_pin skewed skew_row skew_width dep_ver"
SERVER_PID=""

fail() { printf 'FAIL  %s\n' "$*"; [ -n "$SERVER_PID" ] && tail -c 2000 "$WORK/server.log"; stop_server; exit 1; }
say() { printf '%s\n' "$*"; }
stop_server() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait "$SERVER_PID" 2>/dev/null
  SERVER_PID=""
}
trap stop_server EXIT

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

corpus_digest() {
  (
    cd "$CORPUS" || exit 1
    find . -type f -not -path '*/.git/*' -not -name all.config.toml \
      | LC_ALL=C sort \
      | while IFS= read -r file; do printf '%s\n' "$file"; cat "$file"; done
  ) | shasum -a 256 | cut -d' ' -f1
}

manifest_field() { awk -F'\t' -v key="$1" '$1 == key { print $2 }' "$MANIFEST"; }

write_golden() {
  resolve_v5_bin
  git -C "$REPO" diff --quiet -- "$V5_PROGRAM" \
    || say "NOTE  examples/version-skew.dl has uncommitted changes in the source tree"
  SPREFA_CONFIG="$CORPUS/all.config.toml" DL_STATE_DIR="$WORK/state" \
    "$DL_V5_BIN" "$V5_PROGRAM" --db "$WORK/v5.sqlite" \
    >"$WORK/v5.out" 2>"$WORK/v5.err" || fail "v5 run failed: $(tail -10 "$WORK/v5.err")"
  [ -f "$WORK/state/invocations.db" ] \
    || fail "v5 did not write its state under DL_STATE_DIR=$WORK/state; the run was not isolated"
  local v5_repos
  v5_repos="$(sqlite3 "$WORK/v5.sqlite" 'select count(distinct repo) from rel_dep_pin_txt;')"
  [ "$v5_repos" = "$CORPUS_N" ] \
    || fail "v5 saw $v5_repos repos, the pin has $CORPUS_N"

  mkdir -p "$GOLDEN"
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" \
    'select distinct repo, module, ver from rel_dep_pin_txt;'    | LC_ALL=C sort -u >"$GOLDEN/v5.dep_pin.tsv"
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" \
    'select distinct module from rel_skew_txt;'                  | LC_ALL=C sort -u >"$GOLDEN/v5.skewed.tsv"
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" \
    'select distinct module, repo, ver from rel_skew_row_txt;'   | LC_ALL=C sort -u >"$GOLDEN/v5.skew_row.tsv"
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" \
    'select distinct module, repos from rel_skew_width_txt;'     | LC_ALL=C sort -u >"$GOLDEN/v5.skew_width.tsv"
  # The named gap's v5 side, captured so the gate can print what v6 cannot say.
  sqlite3 -noheader -separator '	' "$WORK/v5.sqlite" \
    'select distinct module, lo, hi from rel_dep_ver_txt;'       | LC_ALL=C sort -u >"$GOLDEN/v5.dep_ver.tsv"

  {
    printf 'v5_rev\t%s\n'           "$(git -C "$REPO" rev-parse HEAD)"
    printf 'v5_program\t%s\n'       "examples/version-skew.dl"
    printf 'v5_program_blob\t%s\n'  "$(git -C "$REPO" hash-object "$V5_PROGRAM")"
    printf 'v5_binary_sha256\t%s\n' "$(shasum -a 256 "$DL_V5_BIN" | cut -d' ' -f1)"
    printf 'corpus_repos\t%s\n'     "$CORPUS_N"
    printf 'corpus_sha256\t%s\n'    "$(corpus_digest)"
    printf 'captured_utc\t%s\n'     "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'regenerate\t%s\n'       "$REGEN_CMD"
    for rel in $V5_RELS; do
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
  for rel in $V5_RELS; do
    [ -f "$GOLDEN/v5.$rel.tsv" ] || fail "pinned v5 golden missing $GOLDEN/v5.$rel.tsv; regenerate: $REGEN_CMD"
    want="$(manifest_field "rows_$rel")"
    got="$(wc -l <"$GOLDEN/v5.$rel.tsv" | tr -d ' ')"
    [ "$want" = "$got" ] \
      || fail "pinned v5 golden v5.$rel.tsv has $got rows, MANIFEST rows_$rel says $want (truncated golden); regenerate: $REGEN_CMD"
    cp "$GOLDEN/v5.$rel.tsv" "$WORK/v5.$rel.tsv"
  done

  want="$(manifest_field corpus_repos)"
  [ "$want" = "$CORPUS_N" ] \
    || fail "the pin builds $CORPUS_N repos, the golden was captured against $want; regenerate: $REGEN_CMD"
  want="$(manifest_field corpus_sha256)"
  got="$(corpus_digest)"
  [ "$want" = "$got" ] \
    || fail "1_corpus.sh's corpus MOVED since the v5 golden was captured (golden $want, now $got).
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

bash "$HERE/1_corpus.sh" "$CORPUS" >"$WORK/corpus.log" 2>&1 \
  || fail "corpus build failed: $(cat "$WORK/corpus.log")"
CORPUS_N=4
say "PASS  pinned corpus: $CORPUS_N repos at $CORPUS"

# ── assertion 1: the two regex dialects select the same pin lines ───────────
# v5 uses the rust regex crate through SQLite REGEXP; the v6 leg uses python
# `re` inside parity-grep.py and cannot spell a backslash. Different dialects
# are a confound unless they are shown to agree on THIS corpus, so they are.
V5_PATTERN='(\S+/\S+)\s+(v[0-9]\S*)'
V6_PATTERN='([a-zA-Z0-9._-]+/[a-zA-Z0-9._/-]+)[ ]+(v[0-9][a-zA-Z0-9._-]*)'
python3 - "$CORPUS" "$V5_PATTERN" "$V6_PATTERN" >"$WORK/dialect.txt" <<'PY' || fail "regex dialect probe failed"
import re, sys, pathlib
corpus, v5, v6 = sys.argv[1], re.compile(sys.argv[2]), re.compile(sys.argv[3])
mismatch = []
for gomod in sorted(pathlib.Path(corpus).glob("*/go.mod")):
    for line in gomod.read_text().split("\n"):
        a = [m.groups() for m in v5.finditer(line)]
        b = [m.groups() for m in v6.finditer(line)]
        if a != b:
            mismatch.append((str(gomod), line, a, b))
for row in mismatch:
    print("MISMATCH", row)
print("dialect_mismatches", len(mismatch))
PY
grep -q '^dialect_mismatches 0$' "$WORK/dialect.txt" \
  || fail "the two regex dialects disagree on the corpus: $(cat "$WORK/dialect.txt")"
say "PASS  regex dialects agree on the corpus (v5 rust-crate vs v6 python-re, 0 mismatching lines)"

# ── the v5 leg: the pinned capture, checked in ─────────────────────────────
[ "$WRITE_GOLDEN" = "1" ] && write_golden
read_golden

# ── the v6 leg: the served tsv2 engine ─────────────────────────────────────
export MULTIREPO_GREP="$TSV2/scripts/parity-grep.py"
[ -f "$MULTIREPO_GREP" ] || fail "missing $MULTIREPO_GREP"
TSV2_DB=":memory:" TSV2_PORT="$PORT" MULTIREPO_GREP="$MULTIREPO_GREP" \
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

# One want_repo row per repository, at that repository's own HEAD. This is the
# repo set as DATA -- the difference in kind from v5's config file.
batch=""
for slug in alpha beta gamma shared; do
  rev="$(cd "$CORPUS/$slug" && git rev-parse HEAD)"
  [ -n "$batch" ] && batch="$batch,"
  batch="$batch{\"rel\":\"want_repo\",\"sign\":\"add\",\"row\":[\"$slug\",\"$CORPUS/$slug\",\"$rev\"]}"
done
curl -s -o /dev/null -X POST --data-binary "{\"batch\":[$batch]}" "$BASE/edb/events"

rows_of() { curl -s "$BASE/idb/$1"; }
row_count() {
  rows_of "$1" | python3 -c 'import json,sys
try: print(len(json.load(sys.stdin)["rows"]))
except Exception: print(-1)'
}

# Settle = every repo represented AND four consecutive identical count vectors.
settled=""; last=""; stable=0; deadline=$((SECONDS + 300))
while [ "$SECONDS" -lt "$deadline" ]; do
  now="$(row_count dep_pin):$(row_count skewed):$(row_count skew_row):$(row_count skew_width)"
  if [ "${now%%:*}" -gt 0 ] && [ "$now" = "$last" ]; then
    stable=$((stable + 1))
    [ "$stable" -ge 3 ] && { settled="$now"; break; }
  else
    stable=0
  fi
  last="$now"
  sleep 1
done
[ -n "$settled" ] || fail "v6 never settled (last counts dep_pin:skewed:skew_row:skew_width = $last)"
say "PASS  v6 settled, dep_pin:skewed:skew_row:skew_width = $settled"

dump() { # dump REL COL_INDEXES OUT
  rows_of "$1" | python3 -c '
import json,sys
cols=[int(c) for c in sys.argv[1].split(",")]
rows=json.load(sys.stdin)["rows"]
out={"\t".join(str(r[c]) for c in cols) for r in rows}
sys.stdout.write("".join(line+"\n" for line in sorted(out)))' "$2" | LC_ALL=C sort -u >"$3"
}
# column orders come from the .dl6 decls
dump dep_pin    0,1,2 "$WORK/v6.dep_pin.tsv"
dump skewed     0     "$WORK/v6.skewed.tsv"
dump skew_row   0,1,2 "$WORK/v6.skew_row.tsv"
dump skew_width 0,1   "$WORK/v6.skew_width.tsv"

# ── the grade ──────────────────────────────────────────────────────────────
identical=0; differing=0
for rel in dep_pin skewed skew_row skew_width; do
  if cmp -s "$WORK/v5.$rel.tsv" "$WORK/v6.$rel.tsv"; then
    say "GRADE $rel: BYTE-IDENTICAL ($(wc -l <"$WORK/v5.$rel.tsv" | tr -d ' ') rows)"
    identical=$((identical + 1))
  else
    say "GRADE $rel: differs (v5 $(wc -l <"$WORK/v5.$rel.tsv" | tr -d ' ') rows, v6 $(wc -l <"$WORK/v6.$rel.tsv" | tr -d ' ') rows)"
    differing=$((differing + 1))
  fi
done

say "GAP   dep_ver: NOT EXPRESSIBLE in v6 -- aggregate_operand_not_number(min, _, text)."
say "GAP   dep_ver: v5 derives $(wc -l <"$WORK/v5.dep_ver.tsv" | tr -d ' ') rows this rig cannot compare:"
sed 's/^/GAP     /' "$WORK/v5.dep_ver.tsv"

python3 "$CLASSIFY" "$WORK" "$CORPUS" | tee "$WORK/classification.txt"
grade=${PIPESTATUS[0]}
[ "$grade" = "0" ] || fail "unclassified differences remain (see $WORK/classification.txt)"

stop_server
say "artifacts: $WORK"
say "MULTIREPO CRAWL GRADED: $identical/4 rels byte-identical, $differing classified, 1 named gap (dep_ver), 0 unclassified"
