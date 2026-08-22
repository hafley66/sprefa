#!/usr/bin/env bash
# @comment-ok: the flag contract, the one doc site for this rail's arguments.
# feature-reach.sh -- compile the matrix through the rust door and print it.
# Argument 1 is the tree to read, argument 2 the glob to seed, argument 3 a
# comma-separated root list (with none, a Cargo.toml at the target makes the
# PROGRAM derive its roots from `cargo metadata`).
#   bash v6/dl/reach/feature-reach.sh ~/projects/hafley-rs 'crates/*/src/*.rs'
#   bash v6/dl/reach/feature-reach.sh --check
# In a git pathspec `*` crosses `/` but `**` demands a directory level, so
# `crates/*/src/*.rs` matches 83 files where `crates/*/src/**/*.rs` matches 17.
# `/scip/call` is OFF unless FEATURE_REACH_SCIP=1. Over a real corpus the index
# resolve does not finish: failure-modes.md entry 59, `site_occurrence` walk.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6="$(cd "$HERE/../.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"
ENGINE="$V6/sprefa-engine-rs"
TAB="$(printf '\t')"
RELS='cargo_root_count,reach_root_not_a_source,feature,feature_reach,unreachable_feature,entry_reach_summary,value_reach'

fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/feature-reach.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

build() {
  swipl -q -l "$V6/prolog/compile.pl" -l "$V6/prolog/emit_rust.pl" \
    -g "compile_dl6('$HERE/feature-reach.dl6','$WORK/reach.rs',[emitter(emit_rust:emit_program)])" -g halt \
    >"$WORK/compile.log" 2>&1 || fail "compile: $(tail -20 "$WORK/compile.log")"
  cargo build --release --quiet --manifest-path "$ENGINE/Cargo.toml" --bin emit_rust_harness \
    >"$WORK/build.log" 2>&1 || fail "cargo build: $(tail -5 "$WORK/build.log")"
}

# An index at any of the three locations index_path/2 reads (scip_ensure.rs:701).
has_index() {
  [ -f "$1/index.scip" ] || [ -f "$1/.dl/index.scip" ] || [ -f "$1/.dl/.state/index.scip" ]
}

# run <manifest-dir> <glob> <roots> <scip:0|1> <out.tsv>, from the git toplevel:
# pathspec, extractor read and scip project root are all repository-relative.
run() {
  local manifest glob roots want_scip out repo
  manifest="$(cd "$1" && pwd)" glob="$2" roots="$3" want_scip="$4" out="$5"
  repo="$(git -C "$manifest" rev-parse --show-toplevel)"
  local seeds=(--arrive "want=$glob" --arrive "repo_root=$repo")
  if [ -n "$roots" ]; then
    local root_list root_path
    IFS=',' read -r -a root_list <<<"$roots"
    for root_path in "${root_list[@]}"; do
      [ -n "$root_path" ] && seeds+=(--arrive "root_file=$root_path")
    done
  fi
  if [ -f "$manifest/Cargo.toml" ] && command -v cargo >/dev/null; then
    seeds+=(--arrive "cargo_manifest=$manifest")
  fi
  [ "$want_scip" = 1 ] && seeds+=(--arrive "want_scip=1")
  ( cd "$repo" && DL_ADAPTERS_DIR="$HERE" \
      DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$V6/sprefa-extract/target/release/extract}" \
      "$ENGINE/target/release/emit_rust_harness" "$WORK/reach.rs" "${seeds[@]}" \
      --live-hosts --final-only --final-tsv --final-rels "$RELS" ) \
    >"$out" 2>"$WORK/err" || fail "run: $(tail -20 "$WORK/err")"
  [ -n "${DL_TRACE_SUMMARY:-}" ] && cat "$WORK/err" >&2
  return 0
}

rows_of() { grep "^$1$TAB" "$2" | cut -f2- || true; }

report() {
  local out="$1"
  local roots_derived
  roots_derived="$(rows_of cargo_root_count "$out")"
  [ -n "$roots_derived" ] && printf 'roots from cargo metadata: %s\n' "$roots_derived"
  rows_of reach_root_not_a_source "$out" | while IFS="$TAB" read -r root_path; do
    printf 'WARN root not in the glob: %s\n' "$root_path"
  done
  printf '== feature (a handler a `fn main` names in one hop) ==\n'
  rows_of feature "$out" | while IFS="$TAB" read -r feature_path feature_name; do
    printf '  %s::%s\n' "$feature_path" "$feature_name"
  done
  printf '== feature_reach (entry x feature) ==\n'
  printf '  %-4s %-5s %-6s %s\n' 'able' 'hops' 'via' 'entry -> feature'
  rows_of feature_reach "$out" \
    | while IFS="$TAB" read -r entry feature reachable hops via; do
        printf '  %-4s %-5s %-6s %s -> %s\n' "$reachable" "$hops" "$via" "$entry" "$feature"
      done
  printf '== unreachable_feature (declared by a main, reached by nobody) ==\n'
  rows_of unreachable_feature "$out" | while IFS="$TAB" read -r entry feature; do
    printf '  %s declares %s\n' "$entry" "$feature"
  done
  printf '== entry_reach_summary ==\n'
  rows_of entry_reach_summary "$out" | while IFS="$TAB" read -r entry features defs; do
    printf '  features=%-4s defs=%-5s %s\n' "$features" "$defs" "$entry"
  done
  printf '== value_reach (values the entry hands the feature) ==\n'
  rows_of value_reach "$out" | while IFS="$TAB" read -r entry feature values; do
    printf '  %-4s %s -> %s\n' "$values" "$entry" "$feature"
  done
}

# The diet run and the index run have their own expected files because they
# answer differently, which is the reason both host names exist.
check() {
  local fixture="$WORK/reachcrate"
  local status=0 plane want_scip
  build
  cp -R "$HERE/fixtures/reachcrate" "$fixture"
  git -C "$fixture" init -q
  git -C "$fixture" add -A
  for plane in diet scip nested; do
    want_scip=0
    case "$plane" in
      scip)
        command -v rust-analyzer >/dev/null \
          || { printf 'SKIP  %s: rust-analyzer is not installed\n' "$plane"; continue; }
        want_scip=1
        run "$fixture" 'src/*.rs' '' "$want_scip" "$WORK/$plane.tsv"
        ;;
      # The crate read where it lives, whose cargo workspace root and git
      # repository root differ: failure-modes.md entry 60.
      nested)
        run "$HERE/fixtures/reachcrate" 'v6/dl/reach/fixtures/reachcrate/src/*.rs' \
          '' 0 "$WORK/$plane.tsv"
        ;;
      *) run "$fixture" 'src/*.rs' '' "$want_scip" "$WORK/$plane.tsv" ;;
    esac
    if diff -u "$HERE/fixtures/expected.$plane.tsv" "$WORK/$plane.tsv" >"$WORK/$plane.diff"; then
      printf 'PASS  %s\n' "$plane"
    else
      printf 'FAIL  %s\n%s\n' "$plane" "$(cat "$WORK/$plane.diff")"
      status=1
    fi
  done
  return "$status"
}

if [ "${1:-}" = --check ]; then
  check
  exit $?
fi

TARGET="${1:-$ROOT}"
GLOB="${2:-crates/boop/src/*.rs}"
ROOTS="${3:-}"
REPO="$(git -C "$TARGET" rev-parse --show-toplevel)"
WANT_SCIP="${FEATURE_REACH_SCIP:-0}"
if [ "$WANT_SCIP" = 1 ]; then
  has_index "$REPO" || printf 'WARN no index.scip under %s: the indexer runs first\n' "$REPO" >&2
else
  printf 'scip plane off (FEATURE_REACH_SCIP=1 arms it); via reads diet\n' >&2
fi
build
run "$TARGET" "$GLOB" "$ROOTS" "$WANT_SCIP" "$WORK/final.tsv"
cp "$WORK/final.tsv" "${FEATURE_REACH_TSV:-$WORK/keep.tsv}"
report "$WORK/final.tsv"
