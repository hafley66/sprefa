#!/usr/bin/env bash
# Full verification loop: build, suite with the FSEvents flake policy, rails.
#
# Flake policy: daemon/watcher e2e tests occasionally flake on macOS FSEvents
# timing. A failed test is re-run ALONE; passing solo = flake (reported),
# failing solo = real. Everything is observed, nothing is assumed green.
#
# Rails run on the just-built branch binary with DL_NO_DAEMON=1 and an isolated
# --db so a running daemon can never serve a stale cached program (root = cwd;
# the --root/--no-daemon flags were retired by the de-root arc).
set -uo pipefail
cd "$(dirname "$0")/.."

# Verified-tree stamp: a full green run records a digest of the exact working
# tree it proved (HEAD sha + tracked diff + untracked-file inventory). A re-run
# on an UNCHANGED tree skips straight to the rails — the suite result cannot
# have changed. VERIFY_FORCE=1 overrides. Only THIS script writes the stamp,
# so a skip is always backed by a full run of this same gauntlet.
stamp=.dl/verified-sha
tree_digest=$( { git rev-parse HEAD; git diff HEAD; \
    git ls-files --others --exclude-standard -z | xargs -0 shasum -a 256 2>/dev/null; } \
    | shasum -a 256 | awk '{print $1}')
if [ "${VERIFY_FORCE:-}" != "1" ] \
   && [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$tree_digest" ]; then
  echo "[verify] tree already verified ($tree_digest) — skipping build+suite (VERIFY_FORCE=1 to override)"
else

echo "[verify] cargo build --bin dl"
cargo build --bin dl || exit 1

echo "[verify] cargo test"
suite_log=$(mktemp)
cargo test 2>&1 | tee "$suite_log"
suite_rc=${PIPESTATUS[0]}

if [ "$suite_rc" -ne 0 ]; then
  fails=$(grep -E '^test \S+ \.\.\. FAILED' "$suite_log" | awk '{print $2}' | sort -u)
  if [ -z "$fails" ]; then
    echo "[verify] suite failed with no parsable failing test names" >&2
    exit 1
  fi
  real=0
  for t in $fails; do
    echo "[verify] re-running solo (flake check): $t"
    if perl -e 'alarm 180; exec @ARGV' -- cargo test "$t" >/dev/null 2>&1; then
      echo "[verify] FLAKE (passed solo): $t"
    else
      echo "[verify] REAL FAILURE (failed solo too): $t"
      real=1
    fi
  done
  [ "$real" -ne 0 ] && exit 1
fi

# Suite green: stamp the tree digest so the next run on this tree skips.
echo "$tree_digest" > "$stamp"
fi # verified-tree skip

dl=target/debug/dl
[ -x "$dl" ] || { echo "[verify] cargo build --bin dl (for rails)"; cargo build --bin dl || exit 1; }
echo "[verify] rail: file-size law"
./scripts/filesize-rail.sh || exit 2
echo "[verify] rail: magic-rel audit"
DL_NO_DAEMON=1 "$dl" .dl/magic-rel-audit.dl --db "$(mktemp -d)/rail.sqlite" --check || exit 2
echo "[verify] rail: recompute guard"
DL_NO_DAEMON=1 "$dl" examples/recompute-guard.dl --db "$(mktemp -d)/rail.sqlite" --check || exit 2

echo "[verify] green"
