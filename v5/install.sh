#!/bin/sh
# install.sh — one-line install for `dl` (sprefa).
#
#   curl -fsSL https://raw.githubusercontent.com/hafley66/sprefa/main/install.sh | sh
#
# Downloads the latest prebuilt macOS binary. That is ALL it does by default:
# wiring `dl` into AI coding agents (skills, hooks) mutates your home config,
# so it never happens implicitly — run `dl setup` yourself, or pass --setup to
# opt in here.
set -eu

REPO="hafley66/sprefa"
INSTALLER="https://github.com/$REPO/releases/latest/download/sprefa-dl-installer.sh"

echo "[dl] downloading latest prebuilt binary…"
curl -LsSf "$INSTALLER" | sh

# cargo-dist installs to $CARGO_HOME/bin (install-path = CARGO_HOME).
DL="${CARGO_HOME:-$HOME/.cargo}/bin/dl"
[ -x "$DL" ] || DL="$(command -v dl 2>/dev/null || true)"
if [ -z "${DL:-}" ] || [ ! -x "$DL" ]; then
  echo "[dl] binary installed but not yet on PATH — open a new shell to use it."
  exit 0
fi

case "${1:-}" in
  --setup)
    echo "[dl] wiring agent skill (dl setup)…"
    "$DL" setup
    ;;
  *)
    echo "[dl] installed: $DL"
    echo "[dl] next steps (both optional, both mutate config only when YOU run them):"
    echo "       dl setup              # wire the agent skill into Claude Code / opencode"
    echo "       dl setup --project .  # bootstrap the current repo (.dl/ rails)"
    ;;
esac
