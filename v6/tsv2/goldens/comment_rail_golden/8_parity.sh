#!/usr/bin/env bash
# BEHAVIOR parity, bash tool vs dl6 rail, over synthetic staged diffs in
# throwaway repositories. Cases where the two DISAGREE are expected and named:
# the bash tool reads leading characters, the rail reads comment nodes.
set -uo pipefail

GOLDEN_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2_DIR="$(cd "$GOLDEN_DIR/../.." && pwd)"
V6_DIR="$(cd "$TSV2_DIR/.." && pwd)"
BASH_TOOL="${COMMENT_PROD_BIN:-/Users/chrishafley/projects/claude-research/bin/comment-prod}"
RAIL="$TSV2_DIR/scripts/comment-budget-rail.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/comment-rail-parity.XXXXXX")"
trap 'rm -rf -- "$WORK"' EXIT

FAILED=0
say() { printf '%s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; FAILED=1; }

if [ ! -x "$BASH_TOOL" ]; then
  say "SKIP  bash tool absent at $BASH_TOOL; parity leg cannot run"
  exit 0
fi

new_repo() {
  local name="$1"
  local dir="$WORK/$name"
  mkdir -p "$dir/src"
  git -C "$dir" init -q
  git -C "$dir" config user.email parity@example.com
  git -C "$dir" config user.name parity
  git -C "$dir" commit -q --allow-empty -m base
  printf '%s\n' "$dir"
}

grade() {
  local dir="$1" tool="$2" out status
  out="$WORK/$(basename "$dir").$tool.err"
  if [ "$tool" = bash ]; then
    ( cd "$dir" && "$BASH_TOOL" --cached ) >"$out" 2>&1
  else
    ( cd "$dir" && bash "$RAIL" ) >/dev/null 2>"$out"
  fi
  status=$?
  printf '%s' "$status"
}

verdict_word() { [ "$1" = 0 ] && printf 'CLEAN' || printf 'VIOLATION'; }

case_report() {
  local name="$1" bash_status="$2" rail_status="$3" want="$4"
  local bash_word rail_word
  bash_word="$(verdict_word "$bash_status")"
  rail_word="$(verdict_word "$rail_status")"
  if [ "$rail_word" != "$want" ]; then
    fail "$name: rail said $rail_word, want $want"
    return
  fi
  if [ "$bash_word" = "$rail_word" ]; then
    say "PASS  $name: both $rail_word (agree)"
  else
    say "PASS  $name: bash $bash_word / rail $rail_word (NAMED DIVERGENCE, see README)"
  fi
}

# ── case 1: a plain 3-line comment run in a new file ───────────────────────
dir="$(new_repo violation)"
printf '%s\n' 'export const a = 1;' '// one' '// two' '// three' 'export const b = 2;' \
  >"$dir/src/violation.ts"
git -C "$dir" add src/violation.ts
case_report violation "$(grade "$dir" bash)" "$(grade "$dir" rail)" VIOLATION

# ── case 2: the same run carrying a waiver ─────────────────────────────────
dir="$(new_repo waiver)"
printf '%s\n' 'export const a = 1;' '// @comment-ok: parity fixture' '// two' '// three' \
  'export const b = 2;' >"$dir/src/waiver.ts"
git -C "$dir" add src/waiver.ts
case_report waiver "$(grade "$dir" bash)" "$(grade "$dir" rail)" CLEAN

# ── case 3: the same run on an exempt path ─────────────────────────────────
dir="$(new_repo exempt)"
mkdir -p "$dir/tests"
printf '%s\n' 'export const a = 1;' '// one' '// two' '// three' 'export const b = 2;' \
  >"$dir/tests/exempt.test.ts"
git -C "$dir" add tests/exempt.test.ts
case_report exempt "$(grade "$dir" bash)" "$(grade "$dir" rail)" CLEAN

# ── case 4 (AST divergence): three lines that only LOOK like comments ──────
# A template literal whose lines start at column 0 with `//`. The regex counts
# three comment lines; the grammar calls the whole thing one string.
dir="$(new_repo string_literal)"
printf '%s\n' 'export const banner = `' '// one' '// two' '// three' '`;' \
  >"$dir/src/literal.ts"
git -C "$dir" add src/literal.ts
case_report string_literal "$(grade "$dir" bash)" "$(grade "$dir" rail)" CLEAN

# ── case 5 (AST divergence): a waiver marker inside a string literal ───────
dir="$(new_repo fake_waiver)"
printf '%s\n' 'export const a = 1;' '// one' '// two' '// three' \
  'export const cheat = "@comment-ok: not in a comment";' >"$dir/src/fake.ts"
git -C "$dir" add src/fake.ts
case_report fake_waiver "$(grade "$dir" bash)" "$(grade "$dir" rail)" VIOLATION

# ── case 6 (AST divergence): a 3-line block comment ────────────────────────
dir="$(new_repo block)"
printf '%s\n' 'export const a = 1;' '/* one' 'two' 'three */' 'export const b = 2;' \
  >"$dir/src/block.ts"
git -C "$dir" add src/block.ts
case_report block "$(grade "$dir" bash)" "$(grade "$dir" rail)" VIOLATION

# ── case 7: exactly two comment lines, the threshold boundary ──────────────
dir="$(new_repo boundary)"
printf '%s\n' 'export const a = 1;' '// one' '// two' 'export const b = 2;' \
  >"$dir/src/boundary.ts"
git -C "$dir" add src/boundary.ts
case_report boundary "$(grade "$dir" bash)" "$(grade "$dir" rail)" CLEAN

[ "$FAILED" = 0 ] && say 'COMMENT_RAIL_PARITY HOLDS cases=7'
exit "$FAILED"
