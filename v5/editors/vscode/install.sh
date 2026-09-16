#!/usr/bin/env bash
# One-shot installer for the sprf VS Code extension.
#
# Steps:
#   1. cargo build the sprefa-server binary (v3/crates/server). The
#      same binary serves both --lsp-stdio (this extension) and the
#      HTTP daemon mode used by sprefa-run.
#   2. npm install + tsc compile of the extension client.
#   3. vsce package → local .vsix.
#   4. code --install-extension to load it into VS Code.
#   5. Write the absolute path of the built sprefa-server into VS Code
#      user settings under `sprf.serverPath` so the client does not
#      pick up some random `sprefa-server` from PATH.
#
# Re-runs are idempotent. Pass --release to build the LSP binary in
# release mode.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$here/../.." && pwd)"
v3_dir="$repo_root/v3"

profile="debug"
cargo_flags=()
for arg in "$@"; do
    case "$arg" in
        --release)
            profile="release"
            cargo_flags+=(--release)
            ;;
        *)
            echo "unknown flag: $arg" >&2
            exit 2
            ;;
    esac
done

# 1. Build sprefa-server.
echo "→ building sprefa-server ($profile)" >&2
(
    cd "$v3_dir"
    cargo build -p server --bin sprefa-server "${cargo_flags[@]}"
)
server_bin="$v3_dir/target/$profile/sprefa-server"
[ -x "$server_bin" ] || { echo "missing binary: $server_bin" >&2; exit 1; }

# 2. Compile the TS client.
echo "→ compiling extension client" >&2
cd "$here"
if [ ! -d node_modules ]; then
    npm install
fi
npm run compile

# 3. Package .vsix.
echo "→ packaging .vsix" >&2
need() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1" >&2; exit 1; }; }
need vsce
need code

# vsce rejects a missing repository + LICENSE; skip those checks so the
# local-only build goes through.
vsix="$here/sprf-$(node -p "require('./package.json').version").vsix"
# NOTE: do NOT pass --no-dependencies. vsce bundles prod deps
# (vscode-languageclient) into the .vsix; stripping them makes the
# extension fail at activate() with `Cannot find module
# 'vscode-languageclient/node'`, which kills the LSP client silently —
# syntax highlighting keeps working because tmLanguage doesn't need JS.
vsce package --no-git-tag-version --allow-missing-repository \
    --out "$vsix"

# 4. Install into VS Code.
echo "→ installing into VS Code" >&2
code --install-extension "$vsix" --force

# 5. Pin server path in user settings.
settings="$HOME/Library/Application Support/Code/User/settings.json"
if [ ! -f "$settings" ]; then
    # Cover Linux / Windows-WSL Code layouts as a fallback.
    for alt in \
        "$HOME/.config/Code/User/settings.json" \
        "$HOME/AppData/Roaming/Code/User/settings.json"; do
        [ -f "$alt" ] && { settings="$alt"; break; }
    done
fi
if [ ! -f "$settings" ]; then
    echo "→ VS Code settings.json not found; creating $settings" >&2
    mkdir -p "$(dirname "$settings")"
    printf '{}\n' > "$settings"
fi

echo "→ pinning sprf.serverPath in $settings" >&2
node - "$settings" "$server_bin" <<'NODE'
const fs = require('fs');
const path = require('path');
const [file, lspBin] = process.argv.slice(2);
let raw = fs.readFileSync(file, 'utf8');
// Strip trailing comma + comments enough to parse. VS Code settings
// allow JSONC; this keeps the strip minimal so user content survives.
const stripped = raw
    .replace(/\/\/[^\n]*/g, '')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/,(\s*[}\]])/g, '$1');
let cfg;
try { cfg = JSON.parse(stripped || '{}'); }
catch (e) { console.error('settings.json is not parseable; leaving untouched'); process.exit(1); }
cfg['sprf.serverPath'] = lspBin;
fs.writeFileSync(file, JSON.stringify(cfg, null, 2) + '\n');
NODE

echo "✓ sprf extension installed; sprf.serverPath = $server_bin" >&2
echo "  Reload VS Code windows to pick up the binding." >&2
