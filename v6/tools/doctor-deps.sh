#!/usr/bin/env bash
# Every path dependency on hafley-rs must resolve to the ONE checkout at
# ~/projects/hafley-rs, whatever directory this tree sits in. A worktree at a
# different depth, or a stale symlink beside it, silently builds against an old
# soopy (failure-modes.md, "a symlink older than the crate it names").
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WANT="$(cd "${HAFLEY_RS:-$HOME/projects/hafley-rs}" && pwd -P)"
rc=0
for crate in v6/sprefa-engine-rs v6/sprefa-extract v6/sprefa-store; do
  manifest="$HERE/$crate/Cargo.toml"
  [ -f "$manifest" ] || continue
  for dep in $(grep -oE 'path *= *"[^"]*hafley-rs[^"]*"' "$manifest" | sed -E 's/.*"([^"]*)".*/\1/'); do
    target="$(cd "$HERE/$crate" && cd "$dep" 2>/dev/null && pwd -P || echo MISSING)"
    case "$target" in
      "$WANT"/*) printf 'DEPS OK    %s -> %s\n' "$crate" "$target" ;;
      *) printf 'DEPS STALE %s -> %s (want under %s)\n' "$crate" "$target" "$WANT"; rc=1 ;;
    esac
  done
done
exit $rc
