#!/usr/bin/env bash
# One scenario per process so peak RSS is clean. Fixed rels (n=1000), growing facts.
set -e
cd "$(dirname "$0")/.."
cargo build --release --example salsa_ram -q
echo "-- grow FACTS (files fixed at 1000), watch which strategy grows --"
for m in 1000 5000 10000; do ./target/release/examples/salsa_ram rows   1000 $m; done
for m in 1000 5000 10000; do ./target/release/examples/salsa_ram digest 1000 $m; done
