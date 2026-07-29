#!/usr/bin/env bash
# staleness-gate.sh — ARCH.pl task row `gen_staleness_gate`. Two silent-
# staleness defect classes, both real incidents, neither previously gated:
#
#   (a) GEN-MODULE HALF: v6/tsv2/gen_emitted/*.ts is normally compiler
#       OUTPUT, rewritten every run by v6/tsv2/scripts/sweep.sh for every
#       fixture named "compiled" in v6/prolog/compile/out/manifest.json.
#       door-handwritten.ts is checked in but is NOT a fixture (no manifest
#       row), so sweep.sh never touches it (by design -- it only removes and
#       rewrites the fixture module it is about to regenerate). Nothing else
#       regenerates or diffs it either, so it went silently stale TWICE:
#       once when latest-in-level became a load-time refusal, once when the
#       periods->literals parser rename landed (tick-phase-alignment arc,
#       2026-07-29). Both times a human noticed by hand.
#   (b) BINARY HALF: target/release/dl and
#       v6/sprefa-extract/target/release/extract are built by receipt
#       scripts (flagship-callgraph.sh, extraction-live.sh, lsp-diags.sh)
#       ONLY IF MISSING (`test -x ... || cargo build`). A stale EXISTING
#       binary is never rebuilt automatically: a Jul-20 `dl` binary sat
#       untouched through several landings and silently broke the
#       lsp-diags receipt on an otherwise-current tree until a human
#       rebuilt it by hand (2026-07-29 night close-out, "stale-binary
#       flavor" of this same defect class).
#
# This script is read-only against the tree: it never regenerates a gen
# module or rebuilds a binary itself (the receipt scripts already own that);
# it only refuses to pass silently over a stale artifact, naming the exact
# file and the exact command to fix it.
#
# SABOTAGE RECEIPTS (run by hand against a real green tree, reverted before
# landing -- see the coordinator's task report for the literal exit codes):
#   (a) appended a comment line to the checked-in
#       v6/tsv2/gen_emitted/door-handwritten.ts -> gate exits 1, prints
#       `STALENESS_GATE_FAIL door-handwritten.ts is STALE vs its .dl6
#       source` plus the regen command and a diff. `git checkout --
#       v6/tsv2/gen_emitted/door-handwritten.ts` reverted it, gate green
#       again.
#   (b) `touch src/main.rs` (source newer than the built binary) -> gate
#       exits 1, prints `STALENESS_GATE_FAIL <repo>/target/release/dl is
#       STALE` plus the exact `cargo build --release --bin dl` fix. The
#       binary itself was never touched by the sabotage (only its source),
#       so no rebuild was needed to clear it -- restoring main.rs's mtime
#       with `touch -r` against an unrelated untouched file put the tree
#       back to its pre-sabotage state without paying for a rebuild.
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
  # An associative-array lookup, not `printf ... | grep -qxF`: under
  # `set -o pipefail`, grep closing its read end early (as soon as it finds
  # a match) can SIGPIPE printf before it finishes writing, and pipefail
  # then reports PRINTF's non-zero SIGPIPE status instead of grep's success
  # -- an intermittent false "not found" that bit this exact script during
  # development (module_name near the front of the list, quiet on a later
  # rerun of the identical script). The array sidesteps the pipe entirely.
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
