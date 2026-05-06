#!/usr/bin/env bash
# One-shot installer for the sprefa v4 VS Code extension.
#
# v4 has no LSP server yet (Lane C). This extension is pure declarative:
# language registration + tmGrammar. No TS compile, no runtime deps.
#
# Two install paths are supported:
#   1. `code --install-extension <vsix>` (preferred, requires `vsce`).
#   2. Symlink into ~/.vscode/extensions/ as a fallback.
#
# Re-runs are idempotent.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

ext_id="sprefa.sprefa-v4"
version="$(node -p "require('$here/package.json').version" 2>/dev/null || echo 0.1.0)"

if command -v vsce >/dev/null 2>&1 && command -v code >/dev/null 2>&1; then
    echo "→ packaging .vsix" >&2
    vsix="$here/sprefa-v4-$version.vsix"
    (
        cd "$here"
        vsce package --no-git-tag-version --allow-missing-repository \
            --no-dependencies \
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
