set shell := ["bash", "-euo", "pipefail", "-c"]

# -----------------------------------------------------------------------------
# tree-sitter grammar regeneration
# -----------------------------------------------------------------------------
# parse.md §14.4 — YOLO Phase 1: parser.c is committed per op, regen is manual.
#
#   just regen-host           # v3 host grammar
#   just regen-op glob        # single pattern op
#   just regen-grammars       # all pattern ops under pipeline/src/ops/
#   just regen-all            # host + every pattern op
# -----------------------------------------------------------------------------

regen-host:
    cd v3/crates/tree-sitter-sprefa && tree-sitter generate

regen-op OP:
    cd v3/crates/pipeline/src/ops/{{OP}} && tree-sitter generate

regen-grammars:
    for g in v3/crates/pipeline/src/ops/*/grammar.js; do \
      dir=$(dirname "$g"); \
      echo "regenerating $dir"; \
      (cd "$dir" && tree-sitter generate); \
    done

regen-all: regen-host regen-grammars

# -----------------------------------------------------------------------------
# cargo shortcuts
# -----------------------------------------------------------------------------

test:
    cd v3 && cargo test -p tree-sitter-sprefa -p sprefa_parse -p pipeline

build:
    cd v3 && cargo build --tests
