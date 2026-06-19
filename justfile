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

# Linux perf fixture: v3/tests/smoke/.fixtures/linux is a full kernel source
# tree. It is treated like a git submodule but is intentionally gitignored
# (too big to track; never re-clone it per worktree). It is NOT a real
# submodule. On a fresh checkout run `just fixture-linux` once to provision it.
fixture-linux LINUX="v3/tests/smoke/.fixtures/linux":
    test -n "$(find '{{LINUX}}' -name '*.c' -print -quit 2>/dev/null)" && echo "linux fixture present ($(find '{{LINUX}}' -name '*.c' | wc -l | tr -d ' ') .c files)" || { echo "linux fixture missing — shallow-cloning torvalds/linux (depth 1)…"; rm -rf '{{LINUX}}'; git clone --depth 1 https://github.com/torvalds/linux.git '{{LINUX}}'; echo "provisioned ($(find '{{LINUX}}' -name '*.c' | wc -l | tr -d ' ') .c files, gitignored)"; }

# Linux perf fixture lives in this checkout at v3/tests/smoke/.fixtures/linux.
# Recipe parameters override positionally:
#   just v4-bench-linux "v3/tests/smoke/.fixtures/linux" 'printk($$$)' 8 1 65536
# Running multiple bench commands in parallel pollutes wall time. Run sequentially.
v4-bench-linux LINUX="v3/tests/smoke/.fixtures/linux" PATTERN="printk($$$)" WORKERS="8" TRIALS="3" BATCH="65536":
    ./v4/target/release/v4-bench --root "{{LINUX}}" --workers "{{WORKERS}}" --trials "{{TRIALS}}" --batch "{{BATCH}}" --pattern '{{PATTERN}}' --lang c --mode bare

v4-bench-linux-warm LINUX="v3/tests/smoke/.fixtures/linux" PATTERN="printk($$$)" WORKERS="8" TRIALS="3" BATCH="65536":
    ./v4/target/release/v4-bench --root "{{LINUX}}" --workers "{{WORKERS}}" --trials "{{TRIALS}}" --batch "{{BATCH}}" --pattern '{{PATTERN}}' --lang c --mode bare --warm-page-cache

v4-bench-linux-quick:
    ./v4/target/release/v4-bench --root "v3/tests/smoke/.fixtures/linux" --workers "8" --trials "1" --batch "65536" --pattern 'printk($$$)' --lang c --mode bare

v4-bench-linux-store LINUX="v3/tests/smoke/.fixtures/linux" PATTERN="printk($$$)" WORKERS="8" TRIALS="1" BATCH="65536":
    ./v4/target/release/v4-bench --root "{{LINUX}}" --workers "{{WORKERS}}" --trials "{{TRIALS}}" --batch "{{BATCH}}" --pattern '{{PATTERN}}' --lang c --mode bare --sprf-store

v4-bench-linux-store-warm LINUX="v3/tests/smoke/.fixtures/linux" PATTERN="printk($$$)" WORKERS="8" TRIALS="1" BATCH="65536":
    ./v4/target/release/v4-bench --root "{{LINUX}}" --workers "{{WORKERS}}" --trials "{{TRIALS}}" --batch "{{BATCH}}" --pattern '{{PATTERN}}' --lang c --mode bare --sprf-store --warm-page-cache

v4-bench-linux-sprf LINUX="v3/tests/smoke/.fixtures/linux" BATCH="65536" FACT_DB="/private/tmp/sprefa-v4-bench-linux-facts.db":
    rm -f "{{FACT_DB}}"
    ./v4/target/release/sprefa-run v4/bench/linux.sprf --root "{{LINUX}}" --no-show-rows --telemetry --batch "{{BATCH}}" --fact-db "{{FACT_DB}}"

v4-bench-linux-sprf-telemetry LINUX="v3/tests/smoke/.fixtures/linux" BATCH="65536" FACT_DB="/private/tmp/sprefa-v4-bench-linux-facts.db":
    rm -f "{{FACT_DB}}"
    ./v4/target/release/sprefa-run v4/bench/linux.sprf --root "{{LINUX}}" --no-show-rows --telemetry --batch "{{BATCH}}" --fact-db "{{FACT_DB}}"

v4-bench-linux-sprf-telemetry-batch LINUX="v3/tests/smoke/.fixtures/linux" BATCH="65536" FACT_DB="/private/tmp/sprefa-v4-bench-linux-facts.db":
    rm -f "{{FACT_DB}}"
    ./v4/target/release/sprefa-run v4/bench/linux.sprf --root "{{LINUX}}" --no-show-rows --telemetry --batch "{{BATCH}}" --fact-db "{{FACT_DB}}"

v4-bench-linux-sprf-join LINUX="v3/tests/smoke/.fixtures/linux" BATCH="65536" FACT_DB="/private/tmp/sprefa-v4-bench-linux-join-facts.db":
    rm -f "{{FACT_DB}}"
    ./v4/target/release/sprefa-run v4/bench/linux-join.sprf --root "{{LINUX}}" --no-show-rows --telemetry --batch "{{BATCH}}" --fact-db "{{FACT_DB}}"

v4-bench-linux-sprf-antijoin LINUX="v3/tests/smoke/.fixtures/linux" BATCH="65536" FACT_DB="/private/tmp/sprefa-v4-bench-linux-antijoin-facts.db":
    rm -f "{{FACT_DB}}"
    ./v4/target/release/sprefa-run v4/bench/linux-antijoin.sprf --root "{{LINUX}}" --no-show-rows --telemetry --batch "{{BATCH}}" --fact-db "{{FACT_DB}}"

v4-bench-linux-all LINUX="v3/tests/smoke/.fixtures/linux" PATTERN="printk($$$)" WORKERS="8" TRIALS="1" BATCH="65536":
    ./v4/target/release/v4-bench --root "{{LINUX}}" --workers "{{WORKERS}}" --trials "{{TRIALS}}" --batch "{{BATCH}}" --pattern '{{PATTERN}}' --lang c --mode bare --warm-page-cache
    ./v4/target/release/v4-bench --root "{{LINUX}}" --workers "{{WORKERS}}" --trials "{{TRIALS}}" --batch "{{BATCH}}" --pattern '{{PATTERN}}' --lang c --mode bare --sprf-store --warm-page-cache
    rm -f "/private/tmp/sprefa-v4-bench-linux-all-materialize.db"
    ./v4/target/release/sprefa-run v4/bench/linux.sprf --root "{{LINUX}}" --no-show-rows --telemetry --batch "{{BATCH}}" --fact-db "/private/tmp/sprefa-v4-bench-linux-all-materialize.db"
    rm -f "/private/tmp/sprefa-v4-bench-linux-all-join.db"
    ./v4/target/release/sprefa-run v4/bench/linux-join.sprf --root "{{LINUX}}" --no-show-rows --telemetry --batch "{{BATCH}}" --fact-db "/private/tmp/sprefa-v4-bench-linux-all-join.db"

v4-bench-linux-all-full LINUX="v3/tests/smoke/.fixtures/linux" PATTERN="printk($$$)" WORKERS="8" TRIALS="1" BATCH="65536":
    just v4-bench-linux-all "{{LINUX}}" "{{PATTERN}}" "{{WORKERS}}" "{{TRIALS}}" "{{BATCH}}"
    rm -f "/private/tmp/sprefa-v4-bench-linux-all-antijoin.db"
    ./v4/target/release/sprefa-run v4/bench/linux-antijoin.sprf --root "{{LINUX}}" --no-show-rows --telemetry --batch "{{BATCH}}" --fact-db "/private/tmp/sprefa-v4-bench-linux-all-antijoin.db"

v4-cursor-codec-stress ROWS="706778":
    cargo build --manifest-path v4/Cargo.toml --release --bin cursor-codec-stress
    ./v4/target/release/cursor-codec-stress --rows "{{ROWS}}"

v4-sqlite-queue-stress ROWS="706778" BATCH="65536" DB="/private/tmp/sprefa-sqlite-queue-stress.db":
    cargo build --manifest-path v4/Cargo.toml --release --bin sqlite-queue-stress
    RUST_LOG=effect_runtime::sqlite_queue=info ./v4/target/release/sqlite-queue-stress --rows "{{ROWS}}" --batch "{{BATCH}}" --db "{{DB}}"

v4-sqlite-queue-stress-sweep ROWS="706778" BATCHES="4096,16384,65536,131072" DB="/private/tmp/sprefa-sqlite-queue-stress.db":
    cargo build --manifest-path v4/Cargo.toml --release --bin sqlite-queue-stress
    RUST_LOG=effect_runtime::sqlite_queue=info ./v4/target/release/sqlite-queue-stress --rows "{{ROWS}}" --batches "{{BATCHES}}" --db "{{DB}}"

v3-bench-linux LINUX="v3/tests/smoke/.fixtures/linux" PATTERN="printk($$$)" WORKERS="8" TRIALS="3":
    ./v3/experiments/effect_proof/target/release/ast_grep_v3_bench --root "{{LINUX}}" --workers "{{WORKERS}}" --trials "{{TRIALS}}" --pattern '{{PATTERN}}' --lang c --mode batch

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

v4-architecture-audit: v4-release
    ./v4/target/release/sprefa-run v4/examples/system-architecture-mermaid.sprf --root . --show-rows

v4-docs-toc: v4-release
    ./v4/target/release/sprefa-run v4/examples/docs-toc.sprf --root . --show-rows

v4-docs-generated: v4-architecture-audit v4-docs-toc

v4-release-workflow: v4-test v4-lsp-test v4-release v4-lsp-release v4-vscode-package v4-render-markdown-examples v4-docs-generated

v4-dogfood: v4-flow-smoke v4-lsp-build

# -----------------------------------------------------------------------------
# v5 perf stress fixture
# -----------------------------------------------------------------------------
# Mirrors v3/v4's linux fixture pattern. v5/tests/.fixtures/ holds large real-
# repo checkouts, gitignored, provisioned once on demand (never re-cloned per
# worktree, never tracked). The stress program is v5/bench/stress.dl (scan +
# extract + the type/call/dataflow graphs + closure); the env-gated test is
# tests/perf_stress.rs (skips cleanly when no fixture is present).
#
#   just v5-fixture-rust        # shallow-clone rust-lang/rust-analyzer (depth 1)
#   just v5-bench-rust          # cold stress run, release binary, --profile
#   just v5-bench-self          # no clone: stress against this repo's own .rs
#   just v5-perf-test           # cargo test perf_stress (cold+incremental ratio)
#
# The C path reuses v3/v4's existing fixture: `just fixture-linux` then run the
# v3/v4 bench targets. A C-focused v5 stress is a follow-up.

v5-fixture-rust DIR="v5/tests/.fixtures/rust-analyzer":
    test -n "$(find '{{DIR}}' -name '*.rs' -print -quit 2>/dev/null)" && echo "rust fixture present ($(find '{{DIR}}' -name '*.rs' | wc -l | tr -d ' ') .rs files)" || { echo "rust fixture missing — shallow-cloning rust-lang/rust-analyzer (depth 1)…"; rm -rf '{{DIR}}'; git clone --depth 1 https://github.com/rust-lang/rust-analyzer.git '{{DIR}}'; echo "provisioned ($(find '{{DIR}}' -name '*.rs' | wc -l | tr -d ' ') .rs files, gitignored)"; }

v5-bench-rust DIR="v5/tests/.fixtures/rust-analyzer":
    cargo build --manifest-path v5/Cargo.toml --release --bin dl
    ./v5/target/release/dl v5/bench/stress.dl --root '{{DIR}}' --profile

v5-bench-self:
    cargo build --manifest-path v5/Cargo.toml --release --bin dl
    ./v5/target/release/dl v5/bench/stress.dl --root . --profile

v5-perf-test:
    cargo test --manifest-path v5/Cargo.toml --test perf_stress -- --nocapture
