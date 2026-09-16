#!/usr/bin/env bash
# Refreeze every v8 oracle expected from dl8 itself: bash v8/oracle/refreeze.sh
set -eu
# The per-phase v7 freeze scripts are gone with v7; the JSON files are v8 goldens.
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cargo build --manifest-path "$here/../Cargo.toml" --bin dl8
binary="$(cargo metadata --manifest-path "$here/../Cargo.toml" --format-version 1 --no-deps \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/debug/dl8"
python3 "$here/refreeze.py" "$binary"
