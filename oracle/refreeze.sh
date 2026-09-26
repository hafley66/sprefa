#!/usr/bin/env bash
# Refreeze every oracle expected from the v8 Rust engine: bash oracle/refreeze.sh
set -eu
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 DL8_ENGINE=rust
if [ -z "${CARGO_TARGET_DIR:-}" ]; then
  lane="$(git -C "$here/.." branch --show-current | tr / -)"
  export CARGO_TARGET_DIR="$HOME/.cache/boop/lanes/$lane/target"
fi
binary="$CARGO_TARGET_DIR/debug/dl8"
cargo build -j 4 --manifest-path "$here/../Cargo.toml" --bin dl8
python3 "$here/refreeze.py" "$binary"
