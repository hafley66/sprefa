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

echo "[verify] cargo clippy --all-targets"
# Lint-level baseline lives in Cargo.toml [lints.clippy] — see the comment
# there. All 5 default-active categories (correctness/suspicious/perf/style/
# complexity) currently carry existing findings and are pinned to "warn" so
# this gate is non-fatal on day one; an entry flips to "deny" once its count
# reaches 0, which then makes THIS command fail on any regression.
cargo clippy --all-targets || exit 1

echo "[verify] cargo fmt --check (warn-only, not gating — see TODO below)"
# TODO(fmt-gate): tree is far from rustfmt-clean (328 files as of 2026-07-18,
# agent aaca266d75100c3a5) — three other agents have large in-flight branches
# over src/**, so a repo-wide `cargo fmt` sweep here would conflict with all
# of them. Warn-only until a rustfmt pass lands on a quiet tree; then promote
# the `|| exit 1` below like the clippy step above.
fmt_check_log=$(mktemp)
if ! cargo fmt --check >"$fmt_check_log" 2>&1; then
  # `Diff in <file>:` prints once per changed *line-range*, not once per file —
  # dedupe on the path to get a file count.
  fmt_file_count=$(grep '^Diff in' "$fmt_check_log" | sed -E 's/^Diff in (.*):[0-9]+:.*/\1/' | sort -u | wc -l | tr -d ' ')
  echo "[verify] WARN: cargo fmt --check found $fmt_file_count file(s) out of format (not gating; see TODO(fmt-gate) in this script)"
fi
rm -f "$fmt_check_log"

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
