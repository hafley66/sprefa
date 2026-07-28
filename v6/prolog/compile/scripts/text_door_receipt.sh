#!/usr/bin/env bash
# text_door_receipt.sh: compare the term door and .dl6 text door for the
# fixtures accepted by the current compile sweep.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
swipl -q -l "$HERE/text_door_receipt.pl" -g run -g halt
