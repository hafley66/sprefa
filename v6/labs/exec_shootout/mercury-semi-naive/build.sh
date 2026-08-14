#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
mmc -O5 --make mercury_semi_naive
mv -f mercury_semi_naive mercury-semi-naive
