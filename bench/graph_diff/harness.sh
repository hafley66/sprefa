#!/usr/bin/env bash
# ===========================================================================
# D5.9 rev-axis graph-diff harness  (plans/2026-07-04-d5-rev-aware-extraction.md)
#
# WHAT THIS PROVES
#   The rev-aware graph diff (.dl/graph-diff.dl over the type/call/df extraction
#   twins) reports EXACT, NOISE-FREE deltas from ONE checkout. Four synthetic-PR
#   mutations are applied one at a time to a scratch git repo's WORKING TREE
#   (= head), diffed against its committed base commit (= base), and the
#   node_added/removed + edge_added/removed rows are asserted to the row. The
#   comment-only scenario (S4) asserting ZERO rows is the noise gate: it fails
#   loudly if a line-keyed id ever leaks into the sym-keyed diff.
#
# ORIENTATION (retires the worktree pair)
#   diff_pair("<base sha>", "WORK"): base = the committed HEAD, head = the edited
#   working tree, on ONE checkout. added = present at head not base; removed =
#   present at base not head. The mutation lands in WORK directly, so the verb is
#   NATURAL (adding surfaces as *_added, deleting as *_removed) -- the OLD
#   two-worktree harness inverted it (mutations landed in BASE), that whole
#   ritual (the second `git worktree add ../sprefa-base` + the two-root
#   diff.config.toml + the sprefa-base fast-forward) is GONE.
#
# SELF-CONTAINED
#   Every scenario builds its own throwaway git repo under a mktemp dir with a
#   couple of tiny .rs files, so there is no dependency on the checkout's own
#   source, no SPREFA_CONFIG, no SCIP index. A single trap cleans the scratch.
#
# EXIT CODES
#   0  all four scenarios matched spec
#   1  a hard gate (S1, S2, S3, S4) mismatched  -> real regression
# ===========================================================================
set -uo pipefail

HEAD_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$HEAD_ROOT/target/release/dl"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/d5-graphdiff.XXXXXX")"

HARD_FAIL=0

say()  { printf '%s\n' "$*"; }
hr()   { printf -- '---------------------------------------------------------\n'; }
die()  { say "FATAL: $*"; exit 1; }

cleanup() { rm -rf "$SCRATCH"; }
trap cleanup EXIT

# --- build the release binary if it is missing --------------------------
if [ ! -x "$BIN" ]; then
  say "building $BIN (cargo build --release) ..."
  ( cd "$HEAD_ROOT" && cargo build --release ) || die "cargo build --release failed"
fi
[ -x "$BIN" ] || die "missing built binary $BIN"

# --- the diff program: full graph-diff core, diff_pair(<base sha>, WORK) ---
# Mirrors .dl/graph-diff.dl (types + call edges + the ctor-fill member edge)
# but with the diff_pair fact set to the scenario's base sha vs WORK, written
# fresh per scenario so the base commit is the one under test.
write_diff_prog() {
  local prog_file="$1" base_sha="$2"
  cat > "$prog_file" <<DL
rel diff_pair(base_rev: text, head_rev: text).
diff_pair("$base_sha", "WORK").

rel diff_seen(path: file).
diff_seen(path) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, rev).
diff_seen(path) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, rev).

rel head_rev(rev: text).
head_rev(resolved) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, resolved).
rel base_rev(rev: text).
base_rev(resolved) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, resolved).

rel bare_node(repo: text, sym: text, kind: text).
bare_node(rev, sym, "struct")    <- type_entity_rev(_, sym, _, "struct", _, _, _, rev).
bare_node(rev, sym, "enum")      <- type_entity_rev(_, sym, _, "enum", _, _, _, rev).
bare_node(rev, sym, "trait")     <- type_entity_rev(_, sym, _, "trait", _, _, _, rev).
bare_node(rev, sym, "class")     <- type_entity_rev(_, sym, _, "class", _, _, _, rev).
bare_node(rev, sym, "interface") <- type_entity_rev(_, sym, _, "interface", _, _, _, rev).
bare_node(rev, sym, "function")  <- type_entity_rev(_, sym, _, "function", _, _, _, rev).
bare_node(rev, sym, "method")    <- type_entity_rev(_, sym, _, "method", _, _, _, rev).
bare_node(rev, sym, "const")     <- type_entity_rev(_, sym, _, "const", _, _, _, rev).

rel owner_at_rev(owner_sym: text, name: text, repo: text, rev: text).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*\$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "struct", _, _, _, rev).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*\$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "enum", _, _, _, rev).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*\$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "trait", _, _, _, rev).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*\$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "class", _, _, _, rev).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*\$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "interface", _, _, _, rev).

rel fn_at_rev(fn_sym: text, bare: text, repo: text, rev: text).
fn_at_rev(fn_sym, replace_re(fn_sym, "^[^:]*::", ""), replace_re(fn_sym, "::.*\$", ""), rev) <-
  type_entity_rev(_, fn_sym, _, "function", _, _, _, rev).
fn_at_rev(fn_sym, replace_re(fn_sym, "^[^:]*::", ""), replace_re(fn_sym, "::.*\$", ""), rev) <-
  type_entity_rev(_, fn_sym, _, "method", _, _, _, rev).

bare_node(rev, "\${owner_sym}::field::\${fld}", "field") <-
  df_node_rev(node, "new", ty, _, _, _, rev),
  df_node_repo_rev(node, repo, rev),
  df_field_rev(node, fld, _, rev), fld != "..",
  owner_at_rev(owner_sym, ty, repo, rev).

rel bare_edge(repo: text, a: text, b: text, kind: text).
bare_edge(rev, src, dst, kind) <-
  type_link_rev(src, dst, kind, rev),
  bare_node(rev, src, _),
  bare_node(rev, dst, _).
bare_edge(rev, caller, callee, "call") <-
  call_edge_rev(caller, callee, _, rev),
  bare_node(rev, caller, _),
  bare_node(rev, callee, _).
bare_edge(rev, fn_sym, "\${owner_sym}::field::\${fld}", "fill") <-
  df_node_rev(node, "new", ty, fill_fn, _, _, rev),
  df_node_repo_rev(node, repo, rev),
  df_field_rev(node, fld, _, rev), fld != "..",
  owner_at_rev(owner_sym, ty, repo, rev),
  fn_at_rev(fn_sym, fill_fn, repo, rev).

rel node_added(sym: text, kind: text).
node_added(sym, kind) <-
  head_rev(head), base_rev(base),
  bare_node(head, sym, kind),
  !bare_node(base, sym, kind).
rel node_removed(sym: text, kind: text).
node_removed(sym, kind) <-
  head_rev(head), base_rev(base),
  bare_node(base, sym, kind),
  !bare_node(head, sym, kind).
rel edge_added(a: text, b: text, kind: text).
edge_added(a, b, kind) <-
  head_rev(head), base_rev(base),
  bare_edge(head, a, b, kind),
  !bare_edge(base, a, b, kind).
rel edge_removed(a: text, b: text, kind: text).
edge_removed(a, b, kind) <-
  head_rev(head), base_rev(base),
  bare_edge(base, a, b, kind),
  !bare_edge(head, a, b, kind).
DL
}

# --- helpers ------------------------------------------------------------
mk_repo() {
  local repo="$1"
  mkdir -p "$repo/src"
  git -C "$repo" init -q
  git -C "$repo" config user.email t@example.com
  git -C "$repo" config user.name  T
  git -C "$repo" config commit.gpgsign false
}

cnt()  { sqlite3 "$1" "SELECT count(*) FROM $2;" 2>/dev/null; }
rows() { sqlite3 "$1" "$2" 2>/dev/null | LC_ALL=C sort; }
# strip the leading `<repo-basename>::` prefix so row asserts are repo-agnostic
# (the scratch repo's dir basename prefixes every sym).
bare() { echo "substr($1, instr($1, '::') + 2)"; }

assert_eq() {  # <scenario> <label> <expected> <actual>
  if [ "$3" = "$4" ]; then
    say "  ok   $2 = $4"
  else
    say "  FAIL $2: expected [$3] got [$4]"
    HARD_FAIL=1
  fi
}

assert_rows() {  # <scenario> <label> <db> <sql> <expected-sorted>
  local got; got="$(rows "$3" "$4")"
  if [ "$got" = "$5" ]; then
    say "  ok   $2 rows match"
  else
    say "  FAIL $2 rows differ"
    say "    expected: [$5]"
    say "    got:      [$got]"
    HARD_FAIL=1
  fi
}

# run_scenario <name> <repo>  (expects base already committed + WORK edited)
run_scenario() {
  local name="$1" repo="$2"
  local base_sha db prog
  base_sha="$(git -C "$repo" rev-parse HEAD)"
  db="$SCRATCH/$name.sqlite"; rm -f "$db"
  prog="$SCRATCH/$name.dl"
  write_diff_prog "$prog" "$base_sha"
  "$BIN" "$prog" --root "$repo" --no-daemon --db "$db" >"$SCRATCH/$name.out" 2>&1
}

# =========================================================================
say "D5 rev-axis graph-diff harness"
say "bin     = $BIN"
say "scratch = $SCRATCH"
hr

# ---- S1: move a call site from run() to other() -------------------------
# The call edge moves: exactly one edge added (other->helper), one removed
# (run->helper); no node changes.
say "[S1] move call site run() -> other() over callee helper()"
S1="$SCRATCH/s1"; mk_repo "$S1"
cat > "$S1/src/a.rs" <<'RS'
pub fn helper() {}
pub fn run() { helper(); }
pub fn other() {}
RS
git -C "$S1" add -A && git -C "$S1" commit -q -m base
cat > "$S1/src/a.rs" <<'RS'
pub fn helper() {}
pub fn run() {}
pub fn other() { helper(); }
RS
run_scenario s1 "$S1"
assert_eq S1 "node_added"   0 "$(cnt "$SCRATCH/s1.sqlite" rel_node_added)"
assert_eq S1 "node_removed" 0 "$(cnt "$SCRATCH/s1.sqlite" rel_node_removed)"
assert_eq S1 "edge_added"   1 "$(cnt "$SCRATCH/s1.sqlite" rel_edge_added)"
assert_eq S1 "edge_removed" 1 "$(cnt "$SCRATCH/s1.sqlite" rel_edge_removed)"
assert_rows S1 "edge_added" "$SCRATCH/s1.sqlite" \
  "SELECT $(bare a)||' -> '||$(bare b)||'|'||kind FROM rel_edge_added" \
  "src/a.rs::function::other -> src/a.rs::function::helper|call"
assert_rows S1 "edge_removed" "$SCRATCH/s1.sqlite" \
  "SELECT $(bare a)||' -> '||$(bare b)||'|'||kind FROM rel_edge_removed" \
  "src/a.rs::function::run -> src/a.rs::function::helper|call"
hr

# ---- S2: add a field fill (df family) -----------------------------------
# Add field `level` to Config + fill it at the single ctor. The df-derived
# field node + fill edge surface added; the pre-existing `name` fill is
# unchanged. Exactly one node added (the field) + one edge added (the fill).
say "[S2] add field fill Config.level (df family)"
S2="$SCRATCH/s2"; mk_repo "$S2"
cat > "$S2/src/a.rs" <<'RS'
pub struct Config { pub name: i64 }
pub fn make() -> Config { Config { name: 0 } }
RS
git -C "$S2" add -A && git -C "$S2" commit -q -m base
cat > "$S2/src/a.rs" <<'RS'
pub struct Config { pub name: i64, pub level: i64 }
pub fn make() -> Config { Config { name: 0, level: 0 } }
RS
run_scenario s2 "$S2"
assert_eq S2 "node_removed" 0 "$(cnt "$SCRATCH/s2.sqlite" rel_node_removed)"
assert_eq S2 "edge_removed" 0 "$(cnt "$SCRATCH/s2.sqlite" rel_edge_removed)"
assert_eq S2 "node_added"   1 "$(cnt "$SCRATCH/s2.sqlite" rel_node_added)"
assert_eq S2 "edge_added"   1 "$(cnt "$SCRATCH/s2.sqlite" rel_edge_added)"
assert_rows S2 "node_added" "$SCRATCH/s2.sqlite" \
  "SELECT $(bare sym)||'|'||kind FROM rel_node_added" \
  "src/a.rs::struct::Config::field::level|field"
assert_rows S2 "edge_added" "$SCRATCH/s2.sqlite" \
  "SELECT $(bare a)||' -> '||$(bare b)||'|'||kind FROM rel_edge_added" \
  "src/a.rs::function::make -> src/a.rs::struct::Config::field::level|fill"
hr

# ---- S3: delete a self-contained fn -------------------------------------
# Delete helper()'s def (the call in run() stays, now unresolved). Its fn node
# + its call edge surface removed; nothing added.
say "[S3] delete unique fn helper()"
S3="$SCRATCH/s3"; mk_repo "$S3"
cat > "$S3/src/a.rs" <<'RS'
pub fn helper() {}
pub fn run() { helper(); }
RS
git -C "$S3" add -A && git -C "$S3" commit -q -m base
cat > "$S3/src/a.rs" <<'RS'
pub fn run() { helper(); }
RS
run_scenario s3 "$S3"
assert_eq S3 "node_added"   0 "$(cnt "$SCRATCH/s3.sqlite" rel_node_added)"
assert_eq S3 "edge_added"   0 "$(cnt "$SCRATCH/s3.sqlite" rel_edge_added)"
assert_eq S3 "node_removed" 1 "$(cnt "$SCRATCH/s3.sqlite" rel_node_removed)"
assert_eq S3 "edge_removed" 1 "$(cnt "$SCRATCH/s3.sqlite" rel_edge_removed)"
assert_rows S3 "node_removed" "$SCRATCH/s3.sqlite" \
  "SELECT $(bare sym)||'|'||kind FROM rel_node_removed" \
  "src/a.rs::function::helper|function"
assert_rows S3 "edge_removed" "$SCRATCH/s3.sqlite" \
  "SELECT $(bare a)||' -> '||$(bare b)||'|'||kind FROM rel_edge_removed" \
  "src/a.rs::function::run -> src/a.rs::function::helper|call"
hr

# ---- S4: comment-only edit (the NOISE GATE) -----------------------------
# Add a comment line above run(); every line below shifts. The diff is
# sym-keyed, so it MUST be empty. Any nonzero rel means a line-keyed id leaked.
say "[S4] comment-only edit above run() -> NOISE GATE, expect 0/0/0/0"
S4="$SCRATCH/s4"; mk_repo "$S4"
cat > "$S4/src/a.rs" <<'RS'
pub fn helper() {}
pub fn run() { helper(); }
RS
git -C "$S4" add -A && git -C "$S4" commit -q -m base
cat > "$S4/src/a.rs" <<'RS'
pub fn helper() {}
// d5 noise-gate: comment added above run(); every line below shifts down.
pub fn run() { helper(); }
RS
run_scenario s4 "$S4"
assert_eq S4 "node_added"   0 "$(cnt "$SCRATCH/s4.sqlite" rel_node_added)"
assert_eq S4 "node_removed" 0 "$(cnt "$SCRATCH/s4.sqlite" rel_node_removed)"
assert_eq S4 "edge_added"   0 "$(cnt "$SCRATCH/s4.sqlite" rel_edge_added)"
assert_eq S4 "edge_removed" 0 "$(cnt "$SCRATCH/s4.sqlite" rel_edge_removed)"
if [ "$(cnt "$SCRATCH/s4.sqlite" rel_node_added)$(cnt "$SCRATCH/s4.sqlite" rel_node_removed)$(cnt "$SCRATCH/s4.sqlite" rel_edge_added)$(cnt "$SCRATCH/s4.sqlite" rel_edge_removed)" != "0000" ]; then
  say "  NOISE LEAK — offending rows verbatim:"
  rows "$SCRATCH/s4.sqlite" "SELECT 'node_added',sym,kind FROM rel_node_added"     | sed 's/^/    /'
  rows "$SCRATCH/s4.sqlite" "SELECT 'node_removed',sym,kind FROM rel_node_removed" | sed 's/^/    /'
  rows "$SCRATCH/s4.sqlite" "SELECT 'edge_added',a,b,kind FROM rel_edge_added"     | sed 's/^/    /'
  rows "$SCRATCH/s4.sqlite" "SELECT 'edge_removed',a,b,kind FROM rel_edge_removed" | sed 's/^/    /'
fi
hr

# ---- summary ------------------------------------------------------------
say "SUMMARY"
say "  S1 call-move   : $([ "$HARD_FAIL" = 0 ] && echo PASS || echo 'see above')"
say "  S2 field-fill  : $([ "$HARD_FAIL" = 0 ] && echo PASS || echo 'see above')"
say "  S3 delete-fn   : $([ "$HARD_FAIL" = 0 ] && echo PASS || echo 'see above')"
say "  S4 noise-gate  : $([ "$HARD_FAIL" = 0 ] && echo PASS || echo 'see above')"

if [ "$HARD_FAIL" != 0 ]; then
  say "RESULT: hard gate failure (S1/S2/S3/S4). exit 1"
  exit 1
else
  say "RESULT: all four scenarios matched spec. exit 0"
  exit 0
fi
