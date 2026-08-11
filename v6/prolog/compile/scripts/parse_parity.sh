#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/../../../tools/run-capped.sh"

capped "${PARSE_PARITY_BUDGET_S:-300}" "classic/DCG parse parity" \
  swipl -q -l "$HERE/parse_parity.pl" -g parse_parity:run -g halt
