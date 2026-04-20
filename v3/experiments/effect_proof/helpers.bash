#!/usr/bin/env bash
# Benchmarking and harness-comparison helpers for v3 effect_proof.
#
# Source this file from a shell to get the bench runner set up:
#
#   source v3/experiments/effect_proof/helpers.bash
#   _.sprfv3.bench.help
#
# Every function echoes the command it runs before executing it so the
# shell history is self-documenting. Most accept trailing flags which
# forward to the underlying binary.
#
# The commands here reproduce every measurement in
# `v3/docs/FINDINGS.md` and every comparison table in the session
# doc `chat_log/20260420.0.v3-perf-plugin-synthesis-and-library-split.md`.

_SPRFV3_BENCH_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)"
_SPRFV3_LINUX="${_SPRFV3_BENCH_ROOT}/../v2/tests/smoke/.fixtures/linux"
_SPRFV3_PROBE_ROOT="${_SPRFV3_BENCH_ROOT}/../v2"

_sprfv3_echo_run() { echo "+ $*" >&2; "$@"; }
_sprfv3_need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "helpers: missing $1 on PATH" >&2; return 1;
  }
}

# --------------------------------------------------------------------
# Build
# --------------------------------------------------------------------

# Build all three v3 bench binaries in release mode.
_.sprfv3.bench.build() {
  ( cd "$_SPRFV3_BENCH_ROOT" && \
    _sprfv3_echo_run cargo build --release \
      --bin ast_grep_v3_bench \
      --bin sqlite_v3_bench \
      --bin git_tree_bench )
}

# Build the v2 throughput probe (the baseline harness used by
# FINDINGS §5.1 to produce walk-parallel / hopp-v2 numbers).
_.sprfv3.bench.build-probe() {
  ( cd "$_SPRFV3_PROBE_ROOT" && \
    _sprfv3_echo_run cargo build --example throughput_probe_v2 --profile bench )
}

# Run the tests inside v3 workspace.
_.sprfv3.bench.test() {
  ( cd "$_SPRFV3_BENCH_ROOT" && _sprfv3_echo_run cargo test "$@" )
}

# --------------------------------------------------------------------
# Reference baselines (external tools on the linux kernel fixture)
# --------------------------------------------------------------------

# grep -rc printk on kernel (FINDINGS §1 baseline race). ~14 s single-core.
_.sprfv3.bench.grep() {
  _sprfv3_need grep || return 1
  ( cd "$_SPRFV3_LINUX" && \
    _sprfv3_echo_run bash -c "time grep -rc 'printk(' --include='*.c' --include='*.h' . | wc -l" )
}

# ripgrep on kernel. ~1.7 s parallel + SIMD.
_.sprfv3.bench.rg() {
  _sprfv3_need rg || return 1
  ( cd "$_SPRFV3_LINUX" && \
    _sprfv3_echo_run bash -c "time rg -c 'printk\(' -g '*.c' -g '*.h' | wc -l" )
}

# ast-grep CLI on kernel. ~15.7 s (tree-sitter + matcher).
_.sprfv3.bench.ast-grep-cli() {
  _sprfv3_need ast-grep || return 1
  ( cd "$_SPRFV3_LINUX" && \
    _sprfv3_echo_run bash -c "time ast-grep run -l c -p 'printk(\$\$\$)' --json=compact . | wc -l" )
}

# --------------------------------------------------------------------
# v2 throughput probe — baseline harness (FINDINGS §2.1, §2.3, §5.2)
# --------------------------------------------------------------------

# Walk-parallel topology, W workers, on linux kernel, prefilter on.
# Usage: _.sprfv3.bench.probe-walk 8 3
_.sprfv3.bench.probe-walk() {
  local W="${1:-8}"; local T="${2:-3}"; shift 2 2>/dev/null
  _sprfv3_echo_run "$_SPRFV3_PROBE_ROOT/target/release/examples/throughput_probe_v2" \
    --pattern c-printk --lang c --ext c,h --root "$_SPRFV3_LINUX" \
    --topology walk-parallel --workers "$W" --trials "$T" --tsv "$@"
}

# Hopp-v2 topology, W workers. Compare to walk-parallel for topology
# wash result (FINDINGS §2.1).
_.sprfv3.bench.probe-hopp() {
  local W="${1:-8}"; local T="${2:-3}"; shift 2 2>/dev/null
  _sprfv3_echo_run "$_SPRFV3_PROBE_ROOT/target/release/examples/throughput_probe_v2" \
    --pattern c-printk --lang c --ext c,h --root "$_SPRFV3_LINUX" \
    --topology hopp-v2 --workers "$W" \
    --effect 'synthetic:linear,fixed=0,work=real' \
    --arrival dirac --policy opportunistic \
    --cap 4096 --max-batch 64 \
    --trials "$T" --tsv "$@"
}

# Walk-parallel with prefilter DISABLED (FINDINGS §2.3: the 6x lever).
# Expect ~23 s at W=8 vs ~3.6 s with prefilter on.
_.sprfv3.bench.probe-no-prefilter() {
  local W="${1:-8}"; local T="${2:-3}"; shift 2 2>/dev/null
  _sprfv3_echo_run "$_SPRFV3_PROBE_ROOT/target/release/examples/throughput_probe_v2" \
    --pattern c-printk --lang c --ext c,h --root "$_SPRFV3_LINUX" \
    --topology walk-parallel --workers "$W" --trials "$T" \
    --no-prefilter --tsv "$@"
}

# Phase breakdown mode: serial per-file timing, prints read / parse /
# match percentiles + ts-only parse (FINDINGS §2.3 follow-up).
_.sprfv3.bench.probe-breakdown() {
  _sprfv3_echo_run "$_SPRFV3_PROBE_ROOT/target/release/examples/throughput_probe_v2" \
    --pattern c-printk --lang c --ext c,h --root "$_SPRFV3_LINUX" \
    --topology walk-parallel --workers 1 --breakdown "$@"
}

# --------------------------------------------------------------------
# v3 bench: ast-grep via ctx.put (FINDINGS §5)
# --------------------------------------------------------------------

# Batch-granularity emission: one ctx.put(ScanBatch) per trial.
# Handler does par_iter inside. Parity with raw walk-parallel.
_.sprfv3.bench.ast-grep-batch() {
  local W="${1:-8}"; local T="${2:-3}"; shift 2 2>/dev/null
  _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/ast_grep_v3_bench" \
    --root "$_SPRFV3_LINUX" --workers "$W" --trials "$T" \
    --pattern 'printk($$$)' --lang c --mode batch "$@"
}

# Per-file emission: ctx.put(ScanFile) for every path, 63k puts.
# Demonstrates the ~14% (8.4 us) per-item plugin tax.
_.sprfv3.bench.ast-grep-per-file() {
  local W="${1:-8}"; local CAP="${2:-256}"; local T="${3:-3}"; shift 3 2>/dev/null
  _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/ast_grep_v3_bench" \
    --root "$_SPRFV3_LINUX" --workers "$W" --trials "$T" \
    --pattern 'printk($$$)' --lang c --mode per-file --cap "$CAP" "$@"
}

# --------------------------------------------------------------------
# v3 bench: sqlite write leg (FINDINGS §6)
# --------------------------------------------------------------------

# Full extract + insert pipeline.
# Usage: _.sprfv3.bench.sqlite 16 8 3   (cap, max_batch, trials)
_.sprfv3.bench.sqlite() {
  local CAP="${1:-16}"; local MB="${2:-8}"; local T="${3:-3}"; shift 3 2>/dev/null
  _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/sqlite_v3_bench" \
    --root "$_SPRFV3_LINUX" --workers 8 --trials "$T" \
    --chunk 256 --cap "$CAP" --max-batch "$MB" "$@"
}

# Extract only: regex scan, no plugin emission. Isolates scan cost.
_.sprfv3.bench.sqlite-scan-only() {
  _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/sqlite_v3_bench" \
    --root "$_SPRFV3_LINUX" --workers 8 --trials 3 --scan-only "$@"
}

# Extract + plugin emission, writer is a no-op. Isolates plugin cost
# (measured as noise; see FINDINGS §6.1).
_.sprfv3.bench.sqlite-skip-insert() {
  _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/sqlite_v3_bench" \
    --root "$_SPRFV3_LINUX" --workers 8 --trials 3 \
    --chunk 256 --cap 16 --max-batch 8 --skip-insert "$@"
}

# --------------------------------------------------------------------
# v3 bench: git blob walk (FINDINGS §6.5)
# --------------------------------------------------------------------

# git2 backend + shell-out backend head-to-head. 2.1x win for shell.
_.sprfv3.bench.git() {
  local T="${1:-3}"; shift 1 2>/dev/null
  _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/git_tree_bench" \
    --repo "$_SPRFV3_LINUX" --trials "$T" --needle printk "$@"
}

# git2 only.
_.sprfv3.bench.git-git2() {
  _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/git_tree_bench" \
    --repo "$_SPRFV3_LINUX" --trials 3 --needle printk --modes git2
}

# Shell-out only.
_.sprfv3.bench.git-shell() {
  _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/git_tree_bench" \
    --repo "$_SPRFV3_LINUX" --trials 3 --needle printk --modes shell
}

# --------------------------------------------------------------------
# Sweeps (the exploratory runs that produced the flat curves in §5.1)
# --------------------------------------------------------------------

# ast-grep per-file cap/submitters sweep.
_.sprfv3.bench.sweep-ast-grep-per-file() {
  local CAP SUB
  for CAP in 256 1024 4096; do
    for SUB in 16 32 64; do
      echo "=== cap=$CAP submitters=$SUB ==="
      _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/ast_grep_v3_bench" \
        --root "$_SPRFV3_LINUX" --workers 8 --trials 3 \
        --pattern 'printk($$$)' --lang c --mode per-file \
        --cap "$CAP" --submitters "$SUB" 2>&1 | grep -E '^# trial|^v3-'
    done
  done
}

# sqlite cap/max_batch sweep.
_.sprfv3.bench.sweep-sqlite() {
  local CAP MB
  for CAP in 16 32; do
    for MB in 8 16; do
      echo "=== cap=$CAP max_batch=$MB ==="
      _sprfv3_echo_run "$_SPRFV3_BENCH_ROOT/target/release/sqlite_v3_bench" \
        --root "$_SPRFV3_LINUX" --workers 8 --trials 3 \
        --chunk 256 --cap "$CAP" --max-batch "$MB" 2>&1 | grep trial
    done
  done
}

# W=1/4/8 sweep for walk-parallel (reproduces FINDINGS §2.1 table).
_.sprfv3.bench.sweep-probe-walk() {
  local W
  for W in 1 4 8; do
    echo "=== walk-parallel w=$W ==="
    _.sprfv3.bench.probe-walk "$W" 3
  done
}

# --------------------------------------------------------------------
# Head-to-head comparisons (the decisive tables in the session doc)
# --------------------------------------------------------------------

# v2 walk-parallel vs v3 batch, 5 trials each, same W. Proves plugin
# arch parity with raw probe baseline.
_.sprfv3.bench.head-to-head-ast-grep() {
  local W="${1:-8}"
  echo "==== v2 walk-parallel (raw par_iter) W=$W ===="
  _.sprfv3.bench.probe-walk "$W" 5 | grep -E '^# trial|^WalkParallel'
  echo
  echo "==== v3 ctx.put batch (Passthrough + par_iter inside) W=$W ===="
  _.sprfv3.bench.ast-grep-batch "$W" 5 | tail -20
}

# All three workloads through the v3 telemetry surface. Reproduces
# "same table shape across three domains" demo.
_.sprfv3.bench.three-domains() {
  echo "==== ast-grep batch ===="
  _.sprfv3.bench.ast-grep-batch 8 2 2>&1 | tail -15
  echo; echo "==== sqlite extract+insert ===="
  _.sprfv3.bench.sqlite 16 8 2 2>&1 | tail -20
  echo; echo "==== git walk (both backends) ===="
  _.sprfv3.bench.git 2 2>&1 | tail -25
}

# --------------------------------------------------------------------
# Meta
# --------------------------------------------------------------------

_.sprfv3.bench.help() {
  cat <<'EOF'
effect_proof bench helpers

Build:
  _.sprfv3.bench.build              build three v3 benches (release)
  _.sprfv3.bench.build-probe        build v2 throughput probe (bench profile)
  _.sprfv3.bench.test               cargo test inside v3 workspace

Baselines (external tools):
  _.sprfv3.bench.grep               grep -rc printk on linux kernel
  _.sprfv3.bench.rg                 ripgrep on linux kernel
  _.sprfv3.bench.ast-grep-cli       ast-grep CLI on linux kernel

v2 throughput probe (baseline harness):
  _.sprfv3.bench.probe-walk W T             walk-parallel, W workers, T trials
  _.sprfv3.bench.probe-hopp W T             hopp-v2 topology (adaptive queue)
  _.sprfv3.bench.probe-no-prefilter W T     expose the prefilter lever
  _.sprfv3.bench.probe-breakdown            serial phase breakdown

v3 ast-grep (ctx.put surface):
  _.sprfv3.bench.ast-grep-batch W T         one put, par_iter inside handler
  _.sprfv3.bench.ast-grep-per-file W CAP T  one put per file; 8 us tax

v3 sqlite:
  _.sprfv3.bench.sqlite CAP MB T            full extract + insert pipeline
  _.sprfv3.bench.sqlite-scan-only           isolate regex extract cost
  _.sprfv3.bench.sqlite-skip-insert         isolate plugin-emit cost

v3 git:
  _.sprfv3.bench.git T                      git2 + shell head-to-head
  _.sprfv3.bench.git-git2                   git2 only
  _.sprfv3.bench.git-shell                  shell-out only (2.1x faster)

Sweeps:
  _.sprfv3.bench.sweep-ast-grep-per-file    cap × submitters matrix
  _.sprfv3.bench.sweep-sqlite               cap × max_batch matrix
  _.sprfv3.bench.sweep-probe-walk           W in {1,4,8}

Head-to-head:
  _.sprfv3.bench.head-to-head-ast-grep W    probe vs ctx.put, 5 trials
  _.sprfv3.bench.three-domains              ast-grep + sqlite + git via telemetry

Typical drill after build:
  _.sprfv3.bench.build
  _.sprfv3.bench.build-probe
  _.sprfv3.bench.grep
  _.sprfv3.bench.rg
  _.sprfv3.bench.probe-walk 8 3
  _.sprfv3.bench.probe-no-prefilter 8 3     # demonstrate prefilter lever
  _.sprfv3.bench.ast-grep-batch 8 3
  _.sprfv3.bench.sqlite 16 8 3
  _.sprfv3.bench.git 3
  _.sprfv3.bench.head-to-head-ast-grep 8    # parity proof

Paths used:
  linux kernel fixture:   $_SPRFV3_LINUX
  v3 workspace:           $_SPRFV3_BENCH_ROOT
  v2 probe crate:         $_SPRFV3_PROBE_ROOT
EOF
}

# Keep last so `source` trailing exit status is clean.
true
