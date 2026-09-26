#!/usr/bin/env bash
# Refreeze every oracle expected from the v8 Rust engine: bash oracle/refreeze.sh
set -eu
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 DL8_ENGINE=rust
: "${CARGO_TARGET_DIR:?set CARGO_TARGET_DIR to the lane target directory}"
cargo build -j 4 --manifest-path "$here/../Cargo.toml" --bin dl8
binary="$(cargo metadata --manifest-path "$here/../Cargo.toml" --format-version 1 --no-deps \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/debug/dl8"
python3 "$here/refreeze.py" "$binary"
