#!/bin/sh
# install.sh — one-line turnkey install for `dl` (sprefa v5).
#
#   curl -fsSL https://raw.githubusercontent.com/hafley66/sprefa/main/install.sh | sh
#
# Downloads the latest prebuilt macOS binary and wires it into your AI coding
# agents (`dl setup`). Pass --bin-only to skip the agent wiring.
set -eu

REPO="hafley66/sprefa"
INSTALLER="https://github.com/$REPO/releases/latest/download/sprefa-dl-installer.sh"

echo "[dl] downloading latest prebuilt binary…"
curl -LsSf "$INSTALLER" | sh

# cargo-dist installs to $CARGO_HOME/bin (install-path = CARGO_HOME).
DL="${CARGO_HOME:-$HOME/.cargo}/bin/dl"
[ -x "$DL" ] || DL="$(command -v dl 2>/dev/null || true)"
if [ -z "${DL:-}" ] || [ ! -x "$DL" ]; then
  echo "[dl] binary installed but not yet on PATH — open a new shell, then: dl setup"
  exit 0
fi

case "${1:-}" in
  --bin-only) echo "[dl] installed: $DL"; exit 0 ;;
esac

echo "[dl] wiring agent skill (dl setup)…"
"$DL" setup
echo "[dl] done. Inside a repo, bootstrap it with:  dl setup --project ."
