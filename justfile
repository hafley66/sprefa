set shell := ["bash", "-euo", "pipefail", "-c"]
export RUSTC_WRAPPER := ""
export CC := "cc"

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
    cargo test --manifest-path v4/Cargo.toml

v4-target-tests:
    cargo test --manifest-path v4/Cargo.toml --test rule_future_semantics_target -- --ignored

v4-v3-parity-targets:
    cargo test --manifest-path v4/Cargo.toml --test v3_parity_target

v4-flow-smoke:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/lsp-flow-smoke.sprf --show-rows

v4-dev-missing-hook:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/dev-missing-frontend-hook.sprf --show-rows

v4-dev-doc-drift:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/dev-doc-drift.sprf --show-rows --root .

v4-dev-git-todo-index:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/dev-git-todo-index.sprf --show-rows --root .

v4-dev-rust-panic-map:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/dev-rust-panic-map.sprf --show-rows --root .

v4-dev-dogfood: v4-dev-missing-hook v4-dev-doc-drift v4-dev-git-todo-index v4-dev-rust-panic-map

v4-cli-smoke:
    cargo test --manifest-path v4/Cargo.toml --test sprefa_run_cli_smoke -- --nocapture

v4-config-test:
    cargo test --manifest-path v4/Cargo.toml config::tests
    cargo test --manifest-path v4/Cargo.toml parse_args_uses_config_defaults
    cargo test --manifest-path v4/Cargo.toml parse_args_cli_overrides_config_defaults

v4-run-with-config CONFIG="v4/examples/sprefa.config.example.toml" FILE="v4/examples/dev-missing-frontend-hook.sprf":
    mkdir -p /tmp/sprefa
    SPREFA_CONFIG="{{CONFIG}}" cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- "{{FILE}}" --show-rows

v4-ghcache-test:
    cargo test --manifest-path v4/Cargo.toml --test git_watch_dirty_smoke --features ghcache

v4-no-ghcache-test:
    cargo test --manifest-path v4/Cargo.toml --no-default-features --lib --bin sprefa-daemon --bin sprefa-run

v4-lsp-build:
    cargo build --manifest-path v4/crates/sprefa-lsp/Cargo.toml --bin sprefa-lsp

v4-lsp-test:
    cargo test --manifest-path v4/crates/sprefa-lsp/Cargo.toml

v4-app-host-test:
    cargo test --manifest-path v4/app_host/Cargo.toml

v4-vscode-compile:
    cd v4/editors/vscode && npm run compile

v4-vscode-install:
    cd v4/editors/vscode && ./install.sh

v4-dogfood: v4-flow-smoke v4-lsp-build
