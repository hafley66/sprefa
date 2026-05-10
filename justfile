set shell := ["bash", "-euo", "pipefail", "-c"]
export RUSTC_WRAPPER := ""
export CC := "cc"

# -----------------------------------------------------------------------------
# tree-sitter grammar regeneration
# -----------------------------------------------------------------------------
# parse.md §14.4 — YOLO Phase 1: parser.c is committed per op, regen is manual.
#
#   just regen-host           # v3 host grammar
#   just v4-regen-host        # v4 host grammar
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

v4-regen-host:
    cd v4/crates/tree-sitter-sprefa && tree-sitter generate

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

v4-release:
    cargo build --manifest-path v4/Cargo.toml --release

v4-bench-linux LINUX=".bench/linux" PATTERN="printk($$$)" WORKERS="8" TRIALS="3" BATCH="4096":
    ./v4/target/release/v4-bench --root "{{LINUX}}" --workers "{{WORKERS}}" --trials "{{TRIALS}}" --batch "{{BATCH}}" --pattern "{{PATTERN}}" --lang c --mode bare

v4-bench-linux-read LINUX=".bench/linux" PATTERN="printk($$$)" WORKERS="8" TRIALS="3" BATCH="4096":
    ./v4/target/release/v4-bench --root "{{LINUX}}" --workers "{{WORKERS}}" --trials "{{TRIALS}}" --batch "{{BATCH}}" --pattern "{{PATTERN}}" --lang c --mode bare --materialize-read

v4-bench-linux-sprf LINUX=".bench/linux":
    ./v4/target/release/sprefa-run v4/bench/linux.sprf --root "{{LINUX}}" --no-show-rows

v4-release-test: v4-test v4-release

v4-target-tests:
    cargo test --manifest-path v4/Cargo.toml --test rule_future_semantics_target -- --ignored

v4-v3-parity-targets:
    cargo test --manifest-path v4/Cargo.toml --test v3_parity_target

v4-render-markdown-test:
    cargo test --manifest-path v4/Cargo.toml --test v4_parse_smoke host_parse_four_slot_and_diag_shape
    cargo test --manifest-path v4/Cargo.toml --test v3_parity_target render_dot_markdown_alias_writes_file
    cargo test --manifest-path v4/Cargo.toml --test lsp_hover_smoke lsp_hover_inside_render_dot_markdown_body_uses_markdown_provider
    cargo test --offline --manifest-path v4/crates/sprefa-lsp/Cargo.toml render_dot_markdown_body_emits_tokens

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

v4-dev-config-repos:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/config-repos-from-toml.sprf --show-rows --root .

v4-dogfood-comment-region-lsp:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/dogfood-comment-region-lsp.sprf --show-rows --root .

v4-dogfood-rust-doc-lsp:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/dogfood-rust-doc-lsp.sprf --show-rows --root .

v4-dogfood-config-markdown:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/config-repos-markdown.sprf --show-rows --root .

v4-body-rule-rust-panic-map:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/body-rule-rust-panic-map.sprf --show-rows --root .

v4-body-rule-doc-drift:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/body-rule-doc-drift.sprf --show-rows --root .

v4-body-rule-config-markdown:
    cargo run --manifest-path v4/Cargo.toml --bin sprefa-run -- v4/examples/body-rule-config-markdown.sprf --show-rows --root .

v4-body-rule-dogfood: v4-body-rule-rust-panic-map v4-body-rule-doc-drift v4-body-rule-config-markdown

v4-dev-dogfood: v4-dev-missing-hook v4-dev-doc-drift v4-dev-git-todo-index v4-dev-rust-panic-map v4-dev-config-repos v4-dogfood-comment-region-lsp v4-dogfood-rust-doc-lsp v4-dogfood-config-markdown v4-body-rule-dogfood

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

v4-lsp-release:
    cargo build --offline --manifest-path v4/crates/sprefa-lsp/Cargo.toml --release

v4-lsp-test:
    cargo test --offline --manifest-path v4/crates/sprefa-lsp/Cargo.toml

v4-app-host-test:
    cargo test --manifest-path v4/app_host/Cargo.toml

v4-vscode-compile:
    cd v4/editors/vscode && npm run compile

v4-vscode-package: v4-vscode-compile
    cd v4/editors/vscode && vsce package --allow-missing-repository --no-git-tag-version

v4-vscode-install:
    cd v4/editors/vscode && ./install.sh

v4-render-markdown-examples: v4-release
    ./v4/target/release/sprefa-run v4/examples/self-doc-markdown-inject.sprf --root . --show-rows
    ./v4/target/release/sprefa-run v4/examples/openapi-cardinality-markdown.sprf --root . --show-rows
    ./v4/target/release/sprefa-run v4/examples/rust-doc-ast-yaml-map.sprf --root . --show-rows
    ./v4/target/release/sprefa-run v4/examples/render-markdown-subpipe-links.sprf --root . --show-rows

v4-release-workflow: v4-test v4-lsp-test v4-release v4-lsp-release v4-vscode-package v4-render-markdown-examples

v4-dogfood: v4-flow-smoke v4-lsp-build
