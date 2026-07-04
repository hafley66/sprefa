#!/usr/bin/env bash
# ===========================================================================
# D4 synthetic-PR graph-diff harness  (plans/2026-07-03-pr-diff-graph.md)
#
# WHAT THIS PROVES
#   The worktree-pair graph diff (.dl/graph-diff.dl over flow-panel.dl's
#   member_node/member_edge) reports EXACT, NOISE-FREE deltas. Four synthetic
#   PR mutations are applied one at a time to the BASE checkout's working
#   tree, the one-shot diff is run, and the rel_node_added/removed +
#   rel_edge_added/removed rows are asserted to the row. The comment-only
#   scenario (S4) asserting ZERO rows is the noise gate: it fails loudly if a
#   line-keyed id ever leaks into the sym-keyed diff.
#
# ORIENTATION
#   diff_pair("sprefa-base"=BASE, "vscode-flow-panel"=HEAD). added = present
#   in HEAD not BASE; removed = present in BASE not HEAD. Mutations land in
#   BASE's working tree, so the mutation verb INVERTS: adding to BASE surfaces
#   as *_removed; deleting from BASE surfaces as *_added.
#
# PREREQUISITES
#   - the worktree pair exists: HEAD = this worktree, BASE = ~/projects/
#     sprefa-base at the SAME src commit (the identity baseline is exact 0).
#   - the LOCALLY built binary target/release/dl (carries the D5a resolver
#     fix). Do NOT use ~/.cargo/bin/dl (installed 0.4.1, no fix).
#   - BASE clean except the arc's own `.dl/.gitignore` line (D3 SCIP setup).
#
# SCIP-OFF
#   D3 verdict: SCIP is OFF for the diff. This harness moves any .dl/index.scip
#   out of BOTH roots for the duration (suffix .d4harnessbak) and restores it
#   on exit via a trap, so scip_def is 0 during every run.
#
# S2 (df family, WORK-read) — RE-ARMED after the D5b fix
#   The dataflow (df_*) extraction now reads a CONFIG repo (BASE) at its
#   WORKING TREE, like the type and call families, and the new df_node_repo
#   rel attributes each fill to its own repo. So a `fill` edge + its field
#   node added to BASE's working tree surface on the BASE side (mutation
#   inverts to *_removed) instead of being invisible. S2 is a hard gate again:
#   exactly one node_removed (the field) + one edge_removed (the fill).
#
# EXIT CODES
#   0  all four scenarios matched spec
#   1  a hard gate (S1, S2, S3, S4) mismatched  -> real regression
# ===========================================================================
set -uo pipefail

HEAD_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BASE_ROOT="${SPREFA_BASE_ROOT:-$HOME/projects/sprefa-base}"
BIN="$HEAD_ROOT/target/release/dl"
CFG="bench/graph_diff/diff.config.toml"
DLP=".dl/graph-diff.dl"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/d4-graphdiff.XXXXXX")"
SCIP_SUFFIX=".d4harnessbak"

HARD_FAIL=0

say()  { printf '%s\n' "$*"; }
hr()   { printf -- '---------------------------------------------------------\n'; }

cleanup() {
  # restore any index.scip we moved aside
  for r in "$BASE_ROOT" "$HEAD_ROOT"; do
    [ -f "$r/.dl/index.scip$SCIP_SUFFIX" ] && mv -f "$r/.dl/index.scip$SCIP_SUFFIX" "$r/.dl/index.scip"
  done
  rm -rf "$SCRATCH"
}
trap cleanup EXIT

die() { say "FATAL: $*"; exit 1; }

# --- preconditions ------------------------------------------------------
[ -x "$BIN" ]        || die "missing built binary $BIN (build target/release/dl first)"
[ -d "$BASE_ROOT" ]  || die "missing BASE checkout $BASE_ROOT"
[ -f "$HEAD_ROOT/$CFG" ] || die "missing config $CFG"
[ -f "$HEAD_ROOT/$DLP" ] || die "missing $DLP"

cd "$HEAD_ROOT" || die "cannot cd $HEAD_ROOT"

# BASE must be clean apart from the arc's own .dl/.gitignore line.
base_dirt="$(git -C "$BASE_ROOT" status --porcelain | grep -v '^ M \.dl/\.gitignore$' || true)"
if [ -n "$base_dirt" ]; then
  say "FATAL: BASE checkout has unexpected uncommitted changes; refusing to run:"
  say "$base_dirt"
  exit 1
fi

# --- SCIP off -----------------------------------------------------------
for r in "$BASE_ROOT" "$HEAD_ROOT"; do
  [ -f "$r/.dl/index.scip" ] && mv -f "$r/.dl/index.scip" "$r/.dl/index.scip$SCIP_SUFFIX"
done

# --- helpers ------------------------------------------------------------
# run_diff <dbfile>
run_diff() {
  local db="$1"; rm -f "$db"
  SPREFA_CONFIG="$CFG" "$BIN" "$DLP" --root . --no-daemon --db "$db" \
    >"$SCRATCH/run.out" 2>&1
}
cnt()  { sqlite3 "$1" "SELECT count(*) FROM $2;" 2>/dev/null; }
rows() { sqlite3 "$1" "$2" 2>/dev/null | LC_ALL=C sort; }

# assert_eq <scenario> <label> <expected> <actual>
assert_eq() {
  if [ "$3" = "$4" ]; then
    say "  ok   $2 = $4"
  else
    say "  FAIL $2: expected [$3] got [$4]"
    HARD_FAIL=1
  fi
}
# assert_rows <scenario> <label> <db> <sql> <expected-sorted-multiline>
assert_rows() {
  local got; got="$(rows "$3" "$4")"
  if [ "$got" = "$5" ]; then
    say "  ok   $2 rows match"
  else
    say "  FAIL $2 rows differ"
    say "    expected:"; printf '      %s\n' ${5:+"$5"} | sed 's/^      $//'
    say "    got:";      printf '      %s\n' ${got:+"$got"} | sed 's/^      $//'
    HARD_FAIL=1
  fi
}

# patch <file> via python assertion block on stdin; aborts if anchor drift
patch_base() {
  local f="$1"; shift
  ( cd "$BASE_ROOT" && python3 - "$f" ) || die "patch of $f failed (anchor drift?)"
}
restore_base() { git -C "$BASE_ROOT" checkout -- "$1"; }

# =========================================================================
say "D4 graph-diff harness"
say "HEAD = $HEAD_ROOT"
say "BASE = $BASE_ROOT"
say "bin  = $BIN"
say "scratch = $SCRATCH"
hr

# ---- baseline (identity) : exact 0 + SCIP confirmed off -----------------
say "[baseline] identical src, expect 0/0/0/0 and scip_def=0"
run_diff "$SCRATCH/baseline.sqlite"
assert_eq baseline "scip_def"     0 "$(cnt "$SCRATCH/baseline.sqlite" rel_scip_def)"
assert_eq baseline "node_added"   0 "$(cnt "$SCRATCH/baseline.sqlite" rel_node_added)"
assert_eq baseline "node_removed" 0 "$(cnt "$SCRATCH/baseline.sqlite" rel_node_removed)"
assert_eq baseline "edge_added"   0 "$(cnt "$SCRATCH/baseline.sqlite" rel_edge_added)"
assert_eq baseline "edge_removed" 0 "$(cnt "$SCRATCH/baseline.sqlite" rel_edge_removed)"
hr

# ---- S1: move a call site from fn A to fn B -----------------------------
# Move the latest_tag() call out of run() and into print_help(); expect the
# call edge to move fn->latest_tag between the two sides. (BASE mutated, so
# BASE's print_help->latest_tag shows in edge_removed, HEAD's run->latest_tag
# shows in edge_added.)
say "[S1] move call site run()->print_help() over callee latest_tag"
patch_base src/update.rs <<'PY'
import sys; f=sys.argv[1]; s=open(f).read()
a="match latest_tag() {"; b="match Option::<String>::None {"
assert s.count(a)==1, f"S1 anchor A x{s.count(a)}"; s=s.replace(a,b)
c="fn print_help() {\n"; d="fn print_help() {\n    let _ = latest_tag();\n"
assert s.count(c)==1, f"S1 anchor B x{s.count(c)}"; s=s.replace(c,d)
open(f,"w").write(s)
PY
run_diff "$SCRATCH/s1.sqlite"
assert_eq S1 "node_added"   0 "$(cnt "$SCRATCH/s1.sqlite" rel_node_added)"
assert_eq S1 "node_removed" 0 "$(cnt "$SCRATCH/s1.sqlite" rel_node_removed)"
assert_eq S1 "edge_added"   1 "$(cnt "$SCRATCH/s1.sqlite" rel_edge_added)"
assert_eq S1 "edge_removed" 1 "$(cnt "$SCRATCH/s1.sqlite" rel_edge_removed)"
assert_rows S1 "edge_added" "$SCRATCH/s1.sqlite" \
  "SELECT a||'|'||b||'|'||kind FROM rel_edge_added" \
  "src/update.rs::function::run|src/update.rs::function::latest_tag|call"
assert_rows S1 "edge_removed" "$SCRATCH/s1.sqlite" \
  "SELECT a||'|'||b||'|'||kind FROM rel_edge_removed" \
  "src/update.rs::function::print_help|src/update.rs::function::latest_tag|call"
restore_base src/update.rs
hr

# ---- S2: add a field fill (df family) -----------------------------------
# Add field d4probe to CommentLine + fill it at the single ctor site
# (comment_lines). The fill is a df-sourced fact, so this exercises the D5b
# fix: df extraction now reads a CONFIG repo (BASE) at its WORKING TREE, and
# df_node_repo attributes the fill to BASE only (not fanned across every repo
# that declares a CommentLine). Spec: node_removed = the new field node,
# edge_removed = the new fill edge, both BASE-side (mutation inverts to
# *_removed); nothing added.
say "[S2] add field fill CommentLine.d4probe (df family)"
patch_base src/comment.rs <<'PY'
import sys; f=sys.argv[1]; s=open(f).read()
a="    text: &'a str,\n}"; b="    text: &'a str,\n    d4probe: usize,\n}"
assert s.count(a)==1, f"S2 def anchor x{s.count(a)}"; s=s.replace(a,b)
c="CommentLine { line: i, text_off: text.as_ptr() as usize - base, text }"
d="CommentLine { line: i, text_off: text.as_ptr() as usize - base, text, d4probe: 0 }"
assert s.count(c)==1, f"S2 lit anchor x{s.count(c)}"; s=s.replace(c,d)
open(f,"w").write(s)
PY
run_diff "$SCRATCH/s2.sqlite"
assert_eq S2 "node_added"   0 "$(cnt "$SCRATCH/s2.sqlite" rel_node_added)"
assert_eq S2 "edge_added"   0 "$(cnt "$SCRATCH/s2.sqlite" rel_edge_added)"
assert_eq S2 "node_removed" 1 "$(cnt "$SCRATCH/s2.sqlite" rel_node_removed)"
assert_eq S2 "edge_removed" 1 "$(cnt "$SCRATCH/s2.sqlite" rel_edge_removed)"
assert_rows S2 "node_removed" "$SCRATCH/s2.sqlite" \
  "SELECT sym||'|'||kind FROM rel_node_removed" \
  "src/comment.rs::struct::CommentLine::field::d4probe|field"
assert_rows S2 "edge_removed" "$SCRATCH/s2.sqlite" \
  "SELECT a||'|'||b||'|'||kind FROM rel_edge_removed" \
  "src/comment.rs::function::comment_lines|src/comment.rs::struct::CommentLine::field::d4probe|fill"
restore_base src/comment.rs
hr

# ---- S3: delete a small self-contained fn -------------------------------
# Delete uniquely-named latest_tag() def (keep the call in run(), which then
# fails to resolve BASE-side). Expect its fn + ret nodes and its two call
# edges to surface added (present HEAD, absent BASE), and NO row referencing
# latest_tag to survive on the BASE side.
say "[S3] delete unique fn latest_tag()"
patch_base src/update.rs <<'PY'
import sys; f=sys.argv[1]; s=open(f).read()
block='''/// The latest release tag from the GitHub API (`tag_name`), or None on any
/// failure (offline, rate-limited, malformed). Best-effort — `--check` degrades
/// to a clear message rather than an error.
fn latest_tag() -> Option<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let out = Command::new("curl")
        .args(["-LsSf", "-H", "User-Agent: dl-update", &url])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.get("tag_name")?.as_str().map(|s| s.to_string())
}

'''
assert s.count(block)==1, f"S3 block x{s.count(block)}"; s=s.replace(block,"")
open(f,"w").write(s)
PY
run_diff "$SCRATCH/s3.sqlite"
assert_eq S3 "node_removed" 0 "$(cnt "$SCRATCH/s3.sqlite" rel_node_removed)"
assert_eq S3 "edge_removed" 0 "$(cnt "$SCRATCH/s3.sqlite" rel_edge_removed)"
assert_eq S3 "node_added"   2 "$(cnt "$SCRATCH/s3.sqlite" rel_node_added)"
assert_eq S3 "edge_added"   2 "$(cnt "$SCRATCH/s3.sqlite" rel_edge_added)"
assert_rows S3 "node_added" "$SCRATCH/s3.sqlite" \
  "SELECT sym||'|'||kind FROM rel_node_added" \
"src/update.rs::function::latest_tag::ret|ret
src/update.rs::function::latest_tag|function"
assert_rows S3 "edge_added" "$SCRATCH/s3.sqlite" \
  "SELECT a||'|'||b||'|'||kind FROM rel_edge_added" \
"src/update.rs::function::latest_tag|src/rpc.rs::method::Response.ok|call
src/update.rs::function::run|src/update.rs::function::latest_tag|call"
# no BASE-side row may still reference the deleted sym
assert_eq S3 "base nodes w/ latest_tag" 0 \
  "$(cnt "$SCRATCH/s3.sqlite" "rel_bare_node WHERE repo='sprefa-base' AND sym LIKE '%latest_tag%'")"
assert_eq S3 "base edges w/ latest_tag" 0 \
  "$(cnt "$SCRATCH/s3.sqlite" "rel_bare_edge WHERE repo='sprefa-base' AND (a LIKE '%latest_tag%' OR b LIKE '%latest_tag%')")"
restore_base src/update.rs
hr

# ---- S4: comment-only edit (the NOISE GATE) -----------------------------
# Add a comment line above run(); every line below shifts. member_node/
# member_edge are sym-keyed, so the diff MUST be empty. If any of the four
# rels is nonzero, a line-keyed id has leaked into the sym-set diff.
say "[S4] comment-only edit above run() -> NOISE GATE, expect 0/0/0/0"
patch_base src/update.rs <<'PY'
import sys; f=sys.argv[1]; s=open(f).read()
a="pub fn run(args: &[String]) -> Result<i32> {"
b="// d4 noise-gate: comment added above run(); every line below shifts down.\npub fn run(args: &[String]) -> Result<i32> {"
assert s.count(a)==1, f"S4 anchor x{s.count(a)}"; s=s.replace(a,b)
open(f,"w").write(s)
PY
run_diff "$SCRATCH/s4.sqlite"
assert_eq S4 "node_added"   0 "$(cnt "$SCRATCH/s4.sqlite" rel_node_added)"
assert_eq S4 "node_removed" 0 "$(cnt "$SCRATCH/s4.sqlite" rel_node_removed)"
assert_eq S4 "edge_added"   0 "$(cnt "$SCRATCH/s4.sqlite" rel_edge_added)"
assert_eq S4 "edge_removed" 0 "$(cnt "$SCRATCH/s4.sqlite" rel_edge_removed)"
if [ "$(cnt "$SCRATCH/s4.sqlite" rel_node_added)$(cnt "$SCRATCH/s4.sqlite" rel_node_removed)$(cnt "$SCRATCH/s4.sqlite" rel_edge_added)$(cnt "$SCRATCH/s4.sqlite" rel_edge_removed)" != "0000" ]; then
  say "  NOISE LEAK — offending rows verbatim:"
  rows "$SCRATCH/s4.sqlite" "SELECT 'node_added',sym,kind FROM rel_node_added"       | sed 's/^/    /'
  rows "$SCRATCH/s4.sqlite" "SELECT 'node_removed',sym,kind FROM rel_node_removed"   | sed 's/^/    /'
  rows "$SCRATCH/s4.sqlite" "SELECT 'edge_added',a,b,kind FROM rel_edge_added"       | sed 's/^/    /'
  rows "$SCRATCH/s4.sqlite" "SELECT 'edge_removed',a,b,kind FROM rel_edge_removed"   | sed 's/^/    /'
fi
restore_base src/update.rs
hr

# ---- exit gate: BASE clean again ---------------------------------------
base_dirt="$(git -C "$BASE_ROOT" status --porcelain | grep -v '^ M \.dl/\.gitignore$' || true)"
if [ -n "$base_dirt" ]; then
  say "FAIL exit-gate: BASE not restored clean:"
  say "$base_dirt"
  HARD_FAIL=1
else
  say "ok   BASE restored clean (only the arc's .dl/.gitignore remains)"
fi
hr

# ---- summary ------------------------------------------------------------
say "SUMMARY"
say "  S1 call-move   : $([ "$HARD_FAIL" = 0 ] && echo PASS || echo 'see above')"
say "  S2 field-fill  : $([ "$HARD_FAIL" = 0 ] && echo PASS || echo 'see above')"
say "  S3 delete-fn   : $([ "$HARD_FAIL" = 0 ] && echo PASS || echo 'see above')"
say "  S4 noise-gate  : $([ "$HARD_FAIL" = 0 ] && echo PASS || echo 'see above')"

if [ "$HARD_FAIL" != 0 ]; then
  say "RESULT: hard gate failure (S1/S2/S3/S4 or exit-gate). exit 1"
  exit 1
else
  say "RESULT: all four scenarios matched spec. exit 0"
  exit 0
fi
