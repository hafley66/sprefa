#!/usr/bin/env bash
# staleness-gate.sh — reject stale generated modules and binaries.
# Gen-module provenance is checked against manifest and fixture inputs.
# Existing binaries are checked for staleness; missing binaries remain the
# responsibility of the receipt scripts that build them.
#
# This script is read-only against the tree. It names stale artifacts and the
# command owned by the receipt scripts that can refresh them.
#
# ── SABOTAGE RECEIPT (run 2026-08-11, reverted; tree clean after) ─────────────
# @comment-ok: mandated sabotage-receipt documentation, mirrors self-map.sh
#
#   Replaced line 1 of v6/ARCH-MAP.md with `# SABOTAGE PROBE LINE-1 2026-08-11`
#   (was `# v6 architecture map`). Gate output, exactly:
#     STALENESS_GATE_FAIL v6/ARCH-MAP.md is STALE (checked-in does not
#     match self-map regeneration)
#     1c1
#     < # SABOTAGE PROBE LINE-1 2026-08-11
#   Reverted with `git checkout -- v6/ARCH-MAP.md`; line 1 restored.
#   Independent of this probe, the gate FAILS on this base because the
#   checked-in v6/ARCH-MAP.md is already stale against HEAD sources, a
#   separate finding (see the report).
#
# The discriminating part: the change reaches the gate through the same
# self-map.sh entry the production rail uses, so a renderer that faked its
# output would show no diff.
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

# ── (c) ARCH-MAP.md half ──────────────────────────────────────────────────────
# @comment-ok: mandated ARC-MAP staleness gate; self-map entry required by spec
# Regenerate ARCH-MAP.md through self-map.sh's entry and diff the result
# against the checked-in release-gate file. The write destination is a literal
# inside self-map.dl6, so the fresh render lands on the working-tree file; the
# pre-run bytes are snapshotted first and the tree is restored after.

SELF_MAP_SH="$V6_DIR/tsv2/scripts/self-map.sh"
ARCH_MAP="$V6_DIR/ARCH-MAP.md"
ARCH_TMP="${TMP_REGEN:-}"
if [ -z "$ARCH_TMP" ]; then
  ARCH_TMP="$(mktemp -d)"
  trap 'rm -rf "$ARCH_TMP"' EXIT
fi

if [ ! -f "$SELF_MAP_SH" ]; then
  fail "self-map entry missing: $SELF_MAP_SH"
elif [ ! -f "$ARCH_MAP" ]; then
  fail "ARCH-MAP.md missing: $ARCH_MAP"
else
  cp "$ARCH_MAP" "$ARCH_TMP/arch-map.committed"
  if ! bash "$SELF_MAP_SH" >"$ARCH_TMP/self-map.run.log" 2>&1; then
    fail "self-map regeneration failed, ARCH-MAP.md not verified: $(tail -1 "$ARCH_TMP/self-map.run.log")"
  elif ! diff -q "$ARCH_TMP/arch-map.committed" "$ARCH_MAP" >/dev/null 2>&1; then
    fail "v6/ARCH-MAP.md is STALE (checked-in does not match self-map regeneration)"
    printf '  fix: cd %s && just self-map && git add ../ARCH-MAP.md\n' "$V6_DIR" >&2
    diff "$ARCH_TMP/arch-map.committed" "$ARCH_MAP" 2>&1 | head -40 >&2
  fi
  # Restore the pre-run bytes so a stale finding leaves no working-tree diff.
  cp "$ARCH_TMP/arch-map.committed" "$ARCH_MAP"
fi

if [ "$FAIL" -ne 0 ]; then
  exit 1
fi

echo "STALENESS_GATE_OK gen-modules current, binaries current, ARCH-MAP.md current"
