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

# -----------------------------------------------------------------------------
# v4 smoke / dogfood shortcuts
# -----------------------------------------------------------------------------

v4-test:
    CARGO_BUILD_RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml

v4-target-tests:
    CARGO_BUILD_RUSTC_WRAPPER= cargo test --manifest-path v4/Cargo.toml --test rule_future_semantics_target -- --ignored

v4-flow-smoke:
    CARGO_BUILD_RUSTC_WRAPPER= cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/lsp-flow-smoke.sprf --show-rows

v4-lsp-build:
    CARGO_BUILD_RUSTC_WRAPPER= cargo build --manifest-path v4/crates/sprefa-lsp/Cargo.toml --bin sprefa-lsp

v4-lsp-test:
    CARGO_BUILD_RUSTC_WRAPPER= cargo test --manifest-path v4/crates/sprefa-lsp/Cargo.toml

v4-app-host-test:
    CARGO_BUILD_RUSTC_WRAPPER= cargo test --manifest-path v4/app_host/Cargo.toml

v4-vscode-compile:
    cd v4/editors/vscode && npm run compile

v4-vscode-install:
    cd v4/editors/vscode && ./install.sh

v4-dogfood: v4-flow-smoke v4-lsp-build
