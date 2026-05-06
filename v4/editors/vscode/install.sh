#!/usr/bin/env bash
# One-shot installer for the sprefa v4 VS Code extension + LSP server.
#
# Steps:
#   1. cargo install --path ../../crates/sprefa-lsp        (binary on PATH)
#   2. npm install && npm run compile                       (TS client)
#   3. either `code --install-extension <vsix>` (preferred, needs vsce)
#      or symlink into ~/.vscode/extensions/ as fallback.
#
# Re-runs are idempotent.
#
# Override the LSP binary later via the VS Code setting:
#   "sprefa-v4.serverPath": "/abs/path/to/sprefa-lsp"

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
lsp_crate="$here/../../crates/sprefa-lsp"

ext_id="sprefa.sprefa-v4"
version="$(node -p "require('$here/package.json').version" 2>/dev/null || echo 0.1.0)"

# 1. install sprefa-lsp on PATH (~/.cargo/bin by default).
if command -v cargo >/dev/null 2>&1; then
    echo "→ cargo install sprefa-lsp" >&2
    (cd "$lsp_crate" && cargo install --path . --force --quiet)
else
    echo "! cargo not on PATH; skipping sprefa-lsp install. Install Rust first." >&2
fi

# 2. compile TypeScript client.
if command -v npm >/dev/null 2>&1; then
    echo "→ npm install + tsc" >&2
    (cd "$here" && npm install --silent && npm run --silent compile)
else
    echo "! npm not on PATH; skipping TS compile. Extension will fail to load." >&2
fi

# 3. install the extension.
if command -v vsce >/dev/null 2>&1 && command -v code >/dev/null 2>&1; then
    echo "→ packaging .vsix" >&2
    vsix="$here/sprefa-v4-$version.vsix"
    (
        cd "$here"
        vsce package --no-git-tag-version --allow-missing-repository \
            --out "$vsix"
    )
    echo "→ installing into VS Code" >&2
    code --install-extension "$vsix" --force
    echo "✓ $ext_id installed via vsce" >&2
    exit 0
fi

# Fallback: symlink into ~/.vscode/extensions/.
target="$HOME/.vscode/extensions/$ext_id-$version"
echo "→ vsce or code CLI not on PATH; symlinking to $target" >&2
mkdir -p "$(dirname "$target")"
[ -L "$target" ] || [ -e "$target" ] && rm -rf "$target"
ln -s "$here" "$target"
echo "✓ $ext_id symlinked. Reload VS Code windows to pick up the binding." >&2
