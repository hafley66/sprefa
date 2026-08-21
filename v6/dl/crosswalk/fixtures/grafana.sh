#!/usr/bin/env bash
# @comment-ok: the fixture's usage contract and its one network rule.
# grafana.sh -- materialise the real org fixture named by grafana.tsv.
#
#   bash v6/dl/crosswalk/fixtures/grafana.sh [--print-root]
#
# ONE NETWORK CALL PER REPOSITORY, EVER. Each checkout lands under
# ${SPREFA_CACHE:-$HOME/.cache/sprefa}/crosswalk/<org>/<repo> and a second run
# resolves the pinned rev locally and fetches nothing. The whole script is
# capped at 600s.
#
# SOOPY HAS NO CLONE. `soopy::Acquisition` opens an EXISTING repository and its
# operations are FetchRef / FetchTag / Deepen / Unshallow, so creating the
# checkout is `git init` plus `git remote add` here, in a corpus builder, and
# the engine still spells no Git process of its own. `soopy resolve` is what
# verifies the rev landed. The clone signature this wants is a request on the
# PR, not a Command::new("git") inside src/**.
#
# SHALLOW AT THE REV. `git fetch --depth 1 origin <sha>` needs the server to
# serve arbitrary shas; GitHub does. A `partial fetch` failure falls back to a
# full fetch and says so, because a fixture that silently becomes 500 MB is
# worse than a fixture that is loud about it.
#
# SOOPY IS NOT A PATH DEPENDENCY AND NOT OPTIONAL. It is built from its own
# checkout and the path is printed. The rev is what this fixture exists to pin,
# so a run that cannot resolve it through soopy is a FAIL: verifying with the
# tool that wrote the checkout verifies nothing.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TABLE="$HERE/grafana.tsv"
CACHE="${SPREFA_CACHE:-$HOME/.cache/sprefa}/crosswalk"
TAB="$(printf '\t')"
HAFLEY_RS="${HAFLEY_RS:-$HOME/projects/hafley-rs}"

say()  { printf '%s\n' "$*" >&2; }
fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }

if [ "${1:-}" = "--print-root" ]; then
  printf '%s\n' "$CACHE"
  exit 0
fi

# `command -v soopy` is what let a missing binary degrade into a warning.
soopy_bin() {
  if [ -n "${SOOPY_BIN:-}" ]; then
    [ -x "$SOOPY_BIN" ] || fail "SOOPY_BIN=$SOOPY_BIN is not executable"
    printf '%s\n' "$SOOPY_BIN"
    return 0
  fi
  local built="$HAFLEY_RS/target/release/soopy"
  if [ ! -x "$built" ]; then
    [ -f "$HAFLEY_RS/Cargo.toml" ] \
      || fail "no soopy checkout at $HAFLEY_RS; set HAFLEY_RS or SOOPY_BIN"
    say "BUILD soopy from $HAFLEY_RS"
    timeout 900 cargo build --release -p soopy \
      --manifest-path "$HAFLEY_RS/Cargo.toml" >&2 \
      || fail "cargo build -p soopy failed in $HAFLEY_RS"
  fi
  [ -x "$built" ] || fail "cargo built no binary at $built"
  printf '%s\n' "$built"
}

[ -f "$TABLE" ] || fail "no fixture table at $TABLE"
mkdir -p "$CACHE"
SOOPY="$(soopy_bin)"
say "SOOPY $SOOPY"

# The rev check, through the same mechanics the engine reads the checkout with.
resolves() { ( cd "$1" && timeout 60 "$SOOPY" resolve "$2" >/dev/null 2>&1 ); }

rows() { grep -v '^#' "$TABLE" | grep -v '^[[:space:]]*$'; }

acquire() {
  local slug="$1" rev="$2" kind="$3" root="$CACHE/$slug"
  if [ -d "$root/.git" ] && resolves "$root" "$rev"; then
    say "HIT   $slug $rev already present, 0 network"
    return 0
  fi
  mkdir -p "$root"
  if [ ! -d "$root/.git" ]; then
    git -C "$root" init -q
    git -C "$root" remote add origin "https://github.com/$slug.git"
  fi
  if timeout 300 git -C "$root" fetch -q --depth 1 origin "$rev" 2>"$root/.fetch.err"; then
    say "FETCH $slug $rev ($kind) shallow, depth 1"
  else
    say "WARN  $slug: shallow fetch of $rev failed: $(tail -1 "$root/.fetch.err")"
    timeout 300 git -C "$root" fetch -q origin \
      || fail "$slug: full fetch failed too: $(tail -3 "$root/.fetch.err")"
    say "FETCH $slug $rev ($kind) FULL history, the shallow form was declined"
  fi
  git -C "$root" checkout -q --detach "$rev" \
    || fail "$slug: $rev did not check out"
}

started="$(date +%s)"
printf 'slug\trev\trev_kind\tmodule\tglob\tdisk_mb\troot\n'
while IFS="$TAB" read -r slug rev kind module glob; do
  [ -n "$slug" ] || continue
  acquire "$slug" "$rev" "$kind"
  root="$CACHE/$slug"
  resolves "$root" "$rev" || fail "$slug: soopy cannot resolve $rev in $root"
  disk="$(du -sm "$root" | cut -f1)"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$slug" "$rev" "$kind" "$module" "$glob" "$disk" "$root"
done < <(rows)
say "fixture ready in $(( $(date +%s) - started ))s under $CACHE"
