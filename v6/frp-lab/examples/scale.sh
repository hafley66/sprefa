#!/usr/bin/env bash
# One scenario per process so peak RSS is clean (not contaminated across scenarios).
set -e
cd "$(dirname "$0")/.."
cargo build --release --example core_scale -q
for n in 8000 40000 80000; do ./target/release/examples/core_scale full $n; done
for n in 8000 40000 80000; do ./target/release/examples/core_scale zset $n; done
for n in 8000 40000 80000; do ./target/release/examples/core_scale dense $n; done
