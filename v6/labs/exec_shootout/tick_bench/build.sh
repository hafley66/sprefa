#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
rustc --edition=2021 -O 0_driver.rs -o tick_driver
