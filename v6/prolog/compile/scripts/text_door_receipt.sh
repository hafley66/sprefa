#!/usr/bin/env bash
# text_door_receipt.sh: compare the term door and .dl6 text door for the
# fixtures accepted by the current compile sweep.

#
# BUDGET (timeout-gun lane, 2026-07-31). Measured wall on this machine: 7.5s
# for the whole 196-fixture receipt. Default 300s is 40x that, and deliberately
# not 3x: a seven-second baseline has no meaningful multiple -- a tight cap on
# it would be measuring machine load. The honest claim the default encodes is
# "a swipl still running after five minutes is stuck".
# Override with TEXT_DOOR_BUDGET_S.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/../../../tools/run-capped.sh"

capped "${TEXT_DOOR_BUDGET_S:-300}" "the term-door vs text-door swipl receipt" \
  swipl -q -l "$HERE/text_door_receipt.pl" -g run -g halt
