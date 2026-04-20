#!/usr/bin/env bash
# effect_proof drill helpers.
#
#   source v3/experiments/effect_proof/plugins.sh
#   _.sprfv2.expr.plugins.help

_SPRFV2_EXPR_PLUGINS_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

_.sprfv2.expr.plugins.test()  { ( cd "$_SPRFV2_EXPR_PLUGINS_ROOT" && cargo test   "$@" ); }
_.sprfv2.expr.plugins.build() { ( cd "$_SPRFV2_EXPR_PLUGINS_ROOT" && cargo build  "$@" ); }
_.sprfv2.expr.plugins.check() { ( cd "$_SPRFV2_EXPR_PLUGINS_ROOT" && cargo check  "$@" ); }
_.sprfv2.expr.plugins.clean() { ( cd "$_SPRFV2_EXPR_PLUGINS_ROOT" && cargo clean         ); }

_.sprfv2.expr.plugins.watch() {
  (
    cd "$_SPRFV2_EXPR_PLUGINS_ROOT"
    if command -v cargo-watch >/dev/null 2>&1; then
      cargo watch -x test
    else
      echo "cargo-watch not installed; one-shot test run"
      cargo test
    fi
  )
}

_.sprfv2.expr.plugins.list() {
  (
    cd "$_SPRFV2_EXPR_PLUGINS_ROOT"
    echo "# effects"
    ls src/effects/ | grep -v '^mod\.rs$' | sed 's/^/  /'
    echo
    echo "# tests"
    ls tests/ | sed 's/^/  /'
  )
}

_.sprfv2.expr.plugins.count() {
  (
    cd "$_SPRFV2_EXPR_PLUGINS_ROOT"
    printf "%-40s %6s\n" "file" "LoC"
    printf -- "-----------------------------------------------\n"
    for f in src/lib.rs src/effects/*.rs tests/*.rs; do
      [ -f "$f" ] || continue
      printf "%-40s %6s\n" "$f" "$(wc -l < "$f" | tr -d ' ')"
    done
  )
}

# Audit that the authorable surface (effects + tests) stays free of
# type-erasure plumbing. src/lib.rs is the only place it is allowed.
_.sprfv2.expr.plugins.audit() {
  (
    cd "$_SPRFV2_EXPR_PLUGINS_ROOT"
    echo "# audit: Any / downcast / TypeId in authorable surface"
    echo
    echo "## src/effects/  (expect clean)"
    if grep -rn 'downcast\|dyn Any\|TypeId' src/effects/ 2>/dev/null; then
      echo "  ^^ LEAK"
    else
      echo "  clean"
    fi
    echo
    echo "## tests/  (expect clean)"
    if grep -rn 'downcast\|dyn Any\|TypeId' tests/ 2>/dev/null; then
      echo "  ^^ LEAK"
    else
      echo "  clean"
    fi
    echo
    echo "## src/lib.rs  (framework; allowed)"
    grep -cn 'downcast\|dyn Any\|TypeId' src/lib.rs | sed 's/^/  hits: /'
  )
}

# Scaffold a new effect file and register it in mod.rs.
#   _.sprfv2.expr.plugins.add uppercase
_.sprfv2.expr.plugins.add() {
  local name="$1"
  if [ -z "$name" ]; then
    echo "usage: _.sprfv2.expr.plugins.add <effect_name_snake_case>" >&2
    return 1
  fi
  local file="$_SPRFV2_EXPR_PLUGINS_ROOT/src/effects/${name}.rs"
  if [ -e "$file" ]; then
    echo "already exists: $file" >&2
    return 1
  fi
  local pascal
  pascal="$(echo "$name" | awk -F_ '{for (i=1; i<=NF; i++) printf toupper(substr($i,1,1)) substr($i,2)}')"

  cat >"$file" <<EOF
//! Effect kind: ${name}.
//!
//! Scaffold. Fill in request fields, Response type, and Batcher::run.

use crate::{Batcher, BoxFuture, CancellationToken, EffectKind};

pub struct ${pascal} {
    // TODO: request fields
}

impl EffectKind for ${pascal} {
    type Response = (); // TODO: replace with the real response type
}

pub struct Default${pascal}Batcher;

impl Batcher<${pascal}> for Default${pascal}Batcher {
    fn run(&self, _req: ${pascal}, _cancel: CancellationToken) -> BoxFuture<'static, ()> {
        Box::pin(async move { /* TODO */ })
    }
}
EOF
  echo "pub mod ${name};" >>"$_SPRFV2_EXPR_PLUGINS_ROOT/src/effects/mod.rs"
  echo "scaffolded: $file"
  echo "appended  : src/effects/mod.rs"
  echo
  echo "next:"
  echo "  1. edit src/effects/${name}.rs"
  echo "  2. add a test in tests/surface.rs"
  echo "  3. _.sprfv2.expr.plugins.test"
  echo "  4. _.sprfv2.expr.plugins.audit   # src/lib.rs must stay untouched"
}

_.sprfv2.expr.plugins.help() {
  cat <<'EOF'
_.sprfv2.expr.plugins drill helpers:

  .test         cargo test [args]
  .build        cargo build [args]
  .check        cargo check [args]
  .clean        cargo clean
  .watch        cargo-watch -x test (or one-shot fallback)
  .list         list effect files and tests
  .count        LoC per file
  .audit        grep for Any/downcast leaks in effects+tests
  .add <name>   scaffold new effect file + mod.rs line
  .help         this
EOF
}

_.sprfv2.expr.plugins.help
