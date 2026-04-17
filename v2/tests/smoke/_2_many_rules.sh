#!/usr/bin/env bash
# Smoke 2: POST /run against g4_many_rules.sprf. 4 ast rules on the same
# .rs file set — baseline target for parse-cache work. Every file is parsed
# 4 times without a cache; the perf delta after a cache lands is the point.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_common.sh

require_bins
reset_state

[ -d "$REPO_ROOT/.git" ] || { echo "SKIP: $REPO_ROOT is not a git repo"; exit 0; }
SLUG="$(basename "$REPO_ROOT")"

start_server

t0="$(now_ms)"
out="$("$SPREFA_CLI_BIN" run "$V2_DIR/examples/g4_many_rules.sprf" --root "$REPO_ROOT" 2>/dev/null)"
t1="$(now_ms)"
printf '  [%-24s] %5d ms\n' "sprefa run" "$((t1 - t0))" >&2

# Summary only — full output is noisy with 4 rules.
echo "$out" | grep -E '^rule [a-z_]+ — [0-9]+ rows$' || true

# Sanity: expect a header line for each of the four rule names.
for r in pub_fn pub_struct pub_enum impl_block; do
    LC_ALL=C grep -qE "^rule ${r} " <<< "$out" \
        || { echo "missing rule header: ${r}" >&2; exit 1; }
done

echo "OK"
