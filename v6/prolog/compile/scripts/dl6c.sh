#!/usr/bin/env bash
# @comment-ok: the script's single usage site, same family as
# compile/scripts/text_door_receipt.sh's header.
#
#   bash v6/prolog/compile/scripts/dl6c.sh <in.dl6> --target rust|ts --out <dir>
#
# Same argv and exit codes as dl6c.pl's main/1 (0 compiled, 2 a named
# unsupported construct, 1 anything else) and the same bytes compile_dl6/3
# writes. `just build-dl6c` writes the same path; this script differs by
# rebuilding on staleness rather than on every call.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROLOG="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$PROLOG/../.." && pwd)"
STATE="$PROLOG/target/dl6c"

# -print -quit stops at the first newer source, so a fresh state costs one walk.
stale() {
  [ -x "$STATE" ] || return 0
  [ -n "$(find "$PROLOG" -name '*.pl' -newer "$STATE" -print -quit)" ]
}

if stale; then
  mkdir -p "$PROLOG/target"
  sha="$(git -C "$REPO" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  # rm before write: new bytes under the old macOS signature die "Killed: 9".
  rm -f "$STATE"
  DL6C_BUILD_SHA="$sha" swipl -q -l "$PROLOG/dl6c.pl" \
    -g "dl6c_save('$STATE')" -g halt >&2
fi

exec "$STATE" "$@"
