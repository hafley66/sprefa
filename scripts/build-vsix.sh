#!/usr/bin/env bash
# Build the dl LSP VS Code extension VSIX at the crate version, to a FIXED
# filename (editors/vscode-dl/dl-lsp.vsix) that src/setup.rs embeds via
# include_bytes!. This keeps the extension and the `dl` binary same-versioned:
# Cargo.toml is the single source of truth, stamped into the extension's
# package.json here. build.rs refuses to compile if the two ever disagree, so a
# drifted pair can never be released.
#
# Run this whenever the extension source (editors/vscode-dl/) changes or the
# crate version bumps, then commit the regenerated dl-lsp.vsix + package.json.
# A release cuts them together (see scripts/release.sh).
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(grep -m1 '^version = ' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
ext="editors/vscode-dl"

echo "[build-vsix] stamping $ext/package.json version -> $version"
# Touch only the top-level version line (minimal diff; keeps key order/format).
node -e '
  const fs = require("fs"), f = process.argv[1], v = process.argv[2];
  let s = fs.readFileSync(f, "utf8");
  s = s.replace(/("version":\s*")[^"]+(")/, "$1" + v + "$2");
  fs.writeFileSync(f, s);
' "$ext/package.json" "$version"

(
  cd "$ext"
  [ -d node_modules ] || npm ci || npm install
  npm run compile
  npx vsce package --allow-missing-repository -o dl-lsp.vsix
)

git add -f "$ext/dl-lsp.vsix" "$ext/package.json"
echo "[build-vsix] dl-lsp.vsix built + staged at $version"
