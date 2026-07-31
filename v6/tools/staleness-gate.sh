#!/usr/bin/env bash
# staleness-gate.sh — reject stale generated modules and binaries.
# Gen-module provenance is checked against manifest and fixture inputs.
# Existing binaries are checked for staleness; missing binaries remain the
# responsibility of the receipt scripts that build them.
#
# This script is read-only against the tree. It names stale artifacts and the
# command owned by the receipt scripts that can refresh them.
#
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
V6_DIR="$(cd "$HERE/.." && pwd)"
REPO_ROOT="$(cd "$V6_DIR/.." && pwd)"

FAIL=0

fail() {
  printf 'STALENESS_GATE_FAIL %s\n' "$1" >&2
  FAIL=1
}

# ── (a) gen-module half ─────────────────────────────────────────────────────

GEN_DIR="$V6_DIR/tsv2/gen_emitted"
MANIFEST="$V6_DIR/prolog/compile/out/manifest.json"
COMPILE_DL6_SH="$V6_DIR/prolog/compile/scripts/compile_dl6.sh"
FIXTURES_DIR="$V6_DIR/dl/fixtures"

if [ ! -f "$MANIFEST" ]; then
  fail "manifest missing: $MANIFEST -- run \`cd $V6_DIR && just sweep\` at least once before this gate"
elif [ ! -d "$GEN_DIR" ]; then
  fail "gen_emitted dir missing: $GEN_DIR"
else
  # Use an associative-array lookup so pipefail cannot turn grep's early exit
  # into a false missing-module result.
  declare -A COMPILED_SET=()
  while IFS= read -r name; do
    [ -n "$name" ] && COMPILED_SET["$name"]=1
  done < <(node -e '
    const fs = require("node:fs");
    const manifest = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    for (const entry of manifest) if (entry.bucket === "compiled") console.log(entry.name);
  ' "$MANIFEST")

  TMP_REGEN="$(mktemp -d)"
  trap 'rm -rf "$TMP_REGEN"' EXIT

  for module_path in "$GEN_DIR"/*.ts; do
    [ -e "$module_path" ] || continue
    module_name="$(basename "$module_path" .ts)"

    # A name sweep.sh's manifest owns is regenerated + diffed every `just
    # sweep` run already -- not this gate's job to duplicate that.
    if [ -n "${COMPILED_SET[$module_name]:-}" ]; then
      continue
    fi

    # Non-fixture module. Enumerated provenance, not a hardcoded list: any
    # gen_emitted module whose basename matches a v6/dl/fixtures/<name>.dl6
    # file is assumed to be a hand-written door regen (door-handwritten.ts
    # is the only member of this class today) and is regenerated through
    # the SAME text-door command compile_dl6.sh wraps
    # (swipl ... -g "compile_dl6(Input, Output)"). Anything with neither a
    # manifest row nor a matching .dl6 source is unknown provenance, which
    # is itself a staleness finding -- nothing else can ever check it.
    source_dl6="$FIXTURES_DIR/$module_name.dl6"
    if [ ! -f "$source_dl6" ]; then
      fail "$module_name.ts has no discoverable source (expected $source_dl6, and it is not a compiled-fixture name in $MANIFEST) -- unknown provenance is itself staleness"
      continue
    fi

    regen_path="$TMP_REGEN/$module_name.ts"
    regen_err="$TMP_REGEN/$module_name.compile.err"
    if ! bash "$COMPILE_DL6_SH" "$source_dl6" "$regen_path" >/dev/null 2>"$regen_err"; then
      fail "$module_name.ts: regen command failed -- bash $COMPILE_DL6_SH $source_dl6 <out>.ts: $(head -1 "$regen_err")"
      continue
    fi

    if ! diff -q "$regen_path" "$module_path" >/dev/null 2>&1; then
      fail "$module_name.ts is STALE vs its .dl6 source ($source_dl6)"
      printf '  fix: bash %s %s %s\n' "$COMPILE_DL6_SH" "$source_dl6" "$module_path" >&2
      diff "$module_path" "$regen_path" 2>&1 | head -20 >&2
    fi
  done
fi

# ── (b) binary half ──────────────────────────────────────────────────────────

check_binary() {
  local binary="$1" src_dir="$2" cargo_toml="$3" rebuild_cmd="$4"

  # Missing = PASS for this gate: the receipt scripts already build a
  # missing binary on demand. Only a stale EXISTING one is this gate's
  # target -- the defect class it never rebuilds is exactly that one.
  [ -f "$binary" ] || return 0

  local newer_src
  newer_src="$(find "$src_dir" -name '*.rs' -newer "$binary" -print -quit 2>/dev/null || true)"

  local newer_toml=""
  if [ -f "$cargo_toml" ] && [ "$cargo_toml" -nt "$binary" ]; then
    newer_toml="$cargo_toml"
  fi

  if [ -n "$newer_src" ] || [ -n "$newer_toml" ]; then
    fail "$binary is STALE (source newer: ${newer_src:-$newer_toml})"
    printf '  fix: %s\n' "$rebuild_cmd" >&2
  fi
}

check_binary "$REPO_ROOT/target/release/dl" "$REPO_ROOT/src" "$REPO_ROOT/Cargo.toml" \
  "cd $REPO_ROOT && cargo build --release --bin dl"

check_binary "$REPO_ROOT/v6/sprefa-extract/target/release/extract" \
  "$REPO_ROOT/v6/sprefa-extract/src" "$REPO_ROOT/v6/sprefa-extract/Cargo.toml" \
  "cd $REPO_ROOT/v6/sprefa-extract && cargo build --release --features cli --bin extract"

if [ "$FAIL" -ne 0 ]; then
  exit 1
fi

echo "STALENESS_GATE_OK gen-modules current, binaries current"
