#!/usr/bin/env bash
# Zero-code interim LSP bridge: v5's `dl --lsp --diag-db <path>` mode boots NO
# engine, it just polls the `diag_v5` view (PRAGMA data_version, 500ms cadence,
# src/lsp.rs run_diag_db_mode) and republishes textDocument/publishDiagnostics.
# The v6 dl server (v6/dl) already creates that exact view in its own db
# (v6/dl/src/5_diag.ts DIAG_V5_VIEW_SQL) — so pointing the v5 binary's
# --diag-db at the v6 server's db file is the whole bridge. No new code in
# either side; this script only resolves the v5 binary and execs it.
#
# Receipt (raw publishDiagnostics exchange, end to end, no editor): 2026-07-27.
# See docs/lsp.md "M5.3 --diag-db mode" for the wire contract.
#
# Editor wiring one-liner (VS Code, or any generic-LSP client): set the
# language server command to `v6/tools/lsp-v5-bridge.sh [db-path]` and the
# workspace's cwd to the SAME directory the v6 `dl serve` process runs from —
# diag_v5.path is relative-to-that-cwd unless absolute (see GOTCHA below); do
# not touch editors/ from here, this is a comment, not a wiring change.
#
# GOTCHA: the v6 server normalizes every `path` column relative to ITS OWN
# process.cwd() (DL_ROOT in v6/dl/src/4_ingest.ts) before writing rel_diag, so
# diag_v5.path is usually a relative path. This script's `dl --lsp --diag-db`
# process resolves a relative diag_v5.path against ITS OWN cwd (src/lsp.rs
# publish_diag_v5_path). The two cwds MUST match (or the v6 server's row
# writer must send absolute paths) or diagnostics land on the wrong file /
# never resolve. Simplest fix: run both processes from the same directory
# (the v6 `dl serve` cwd), which is also the natural editor-workspace-root
# case.
#
# Usage: v6/tools/lsp-v5-bridge.sh [db-path]
#   db-path   sqlite file the v6 `dl serve` process is writing (default
#             ~/.local/state/dl/mvp.sqlite, i.e. v6/dl/src/main.ts's own
#             default when DL_DB_PATH is unset).
#
# Resolves the v5 `dl` binary: DL_BIN env override, then PATH, then
# target/{release,debug}/dl relative to the repo root (this script lives at
# v6/tools/, so the repo root is two levels up).
set -uo pipefail

db_path="${1:-$HOME/.local/state/dl/mvp.sqlite}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

resolve_dl_bin() {
  if [ -n "${DL_BIN:-}" ]; then
    printf '%s\n' "$DL_BIN"
    return 0
  fi
  if command -v dl >/dev/null 2>&1; then
    command -v dl
    return 0
  fi
  for candidate in "$repo_root/target/release/dl" "$repo_root/target/debug/dl"; do
    if [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

dl_bin="$(resolve_dl_bin)" || {
  printf 'lsp-v5-bridge: no dl binary found (checked DL_BIN, PATH, %s/target/{release,debug}/dl)\n' "$repo_root" >&2
  printf '  build one with: cargo build --bin dl --release  (run from %s)\n' "$repo_root" >&2
  exit 1
}

exec "$dl_bin" --lsp --diag-db "$db_path"
