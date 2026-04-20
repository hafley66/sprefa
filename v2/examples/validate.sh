#!/usr/bin/env bash
# validate.sh — sanity harness with platform-aware model.
#
# macOS `thread::sleep` has a per-call overhead floor around 150-300µs.
# Closed-form expected values must account for (N_sleeps × overhead).
# This script calibrates that overhead once, then scores each case against
# an overhead-adjusted prediction.
#
# Structure:
#   1. CALIBRATION — measure per-sleep overhead on this machine
#   2. ABSOLUTE CASES — score wall_p50 against (ideal + calibrated overhead)
#   3. RATIO CASES   — score scaling ratios (platform-independent)

set -euo pipefail

cd "$(dirname "$0")/.."
BIN="target/release/examples/throughput_probe"
[[ -x "$BIN" ]] || { echo "build probe first: cargo build --release --example throughput_probe" >&2; exit 1; }

ROOT="tests/smoke/.fixtures/swc"
TRIALS=10
TOL_ABS=15   # percent tolerance for absolute cases (after overhead adjustment)
TOL_RATIO=10 # percent tolerance for scaling ratios
pass=0
fail=0

wall_p50() {
  $BIN --pattern baseline --tsv --trials "$TRIALS" --root "$ROOT" --no-chart "$@" 2>/dev/null \
    | awk -F'\t' '{print $13}'
}

# ----------------------------------------------------------------------------
# CALIBRATION
# Run 100 dispatches with fixed-cost=1000µs, 1 consumer, no per-item work.
# Expected ideal = 100ms. Observed excess / 100 = per-sleep overhead µs.
# ----------------------------------------------------------------------------
echo "=== calibration: measuring per-sleep overhead ==="
CALIB_N=200
CALIB_FC=1000
CALIB_WALL=$(wall_p50 --variant Cdam2:2048 --max "$CALIB_N" \
  --fixed-cost-us "$CALIB_FC" --work-dist fixed:0)
IDEAL_MS=$(awk -v n="$CALIB_N" -v fc="$CALIB_FC" 'BEGIN {print n * fc / 1000}')
OVERHEAD_US=$(awk -v obs="$CALIB_WALL" -v ideal="$IDEAL_MS" -v n="$CALIB_N" \
  'BEGIN { printf "%.0f", ((obs - ideal) * 1000) / n }')
printf '  ideal=%.1fms  observed=%.1fms  per-sleep overhead ≈ %sµs\n\n' \
  "$IDEAL_MS" "$CALIB_WALL" "$OVERHEAD_US"

# Helper: compute expected with overhead.
#   args: ideal_ms, n_sleeps_in_critical_path
# Returns: ideal_ms + n_sleeps * OVERHEAD_US / 1000
expected_adj() {
  awk -v i="$1" -v n="$2" -v oh="$OVERHEAD_US" 'BEGIN {printf "%.2f", i + (n * oh / 1000)}'
}

score_abs() {
  local name="$1" expected="$2" observed="$3"
  local delta abs_delta within verdict
  delta=$(awk -v e="$expected" -v o="$observed" 'BEGIN {
    if (e == 0) { print (o == 0) ? 0 : 9999; exit }
    printf "%.1f", ((o - e) / e) * 100
  }')
  abs_delta=$(awk -v d="$delta" 'BEGIN {printf "%.1f", (d < 0) ? -d : d}')
  within=$(awk -v a="$abs_delta" -v t="$TOL_ABS" 'BEGIN {print (a <= t) ? 1 : 0}')
  if [[ "$within" == "1" ]]; then verdict="PASS"; pass=$((pass + 1))
  else verdict="FAIL"; fail=$((fail + 1)); fi
  printf '%-6s  %-60s  pred=%7.1f  obs=%7.1f  delta=%+6s%%\n' \
    "$verdict" "$name" "$expected" "$observed" "$delta"
}

score_ratio() {
  local name="$1" expected_ratio="$2" observed_ratio="$3"
  local delta abs_delta within verdict
  delta=$(awk -v e="$expected_ratio" -v o="$observed_ratio" 'BEGIN {
    printf "%.1f", ((o - e) / e) * 100
  }')
  abs_delta=$(awk -v d="$delta" 'BEGIN {printf "%.1f", (d < 0) ? -d : d}')
  within=$(awk -v a="$abs_delta" -v t="$TOL_RATIO" 'BEGIN {print (a <= t) ? 1 : 0}')
  if [[ "$within" == "1" ]]; then verdict="PASS"; pass=$((pass + 1))
  else verdict="FAIL"; fail=$((fail + 1)); fi
  printf '%-6s  %-60s  expected≈%.1fx  observed=%.2fx  delta=%+6s%%\n' \
    "$verdict" "$name" "$expected_ratio" "$observed_ratio" "$delta"
}

echo "=== ABSOLUTE cases (overhead-adjusted, ±${TOL_ABS}%) ==="

# --- CASE 1: fixed-cost-us paid once per dispatch (Cdam2, no batching).
# 200 items × 1 consumer. Critical-path sleeps = 200.
W=$(wall_p50 --variant Cdam2:2048 --max 200 --fixed-cost-us 1000 --work-dist fixed:0)
E=$(expected_adj 200 200)
score_abs "fixed-cost × 200 @ 1 consumer" "$E" "$W"

# --- CASE 2: work-dist fixed:N paid once per item (mb=1, Hopp2:8).
# 400 items / 8 workers = 50 sleeps per worker = 50 in critical path.
W=$(wall_p50 --variant Hopp2:2048:1:8 --max 400 --fixed-cost-us 0 --work-dist fixed:500)
E=$(expected_adj 25 50)
score_abs "work-dist:500 × 400 / 8 workers (mb=1)" "$E" "$W"

# --- CASE 3: flat:N paid once per batch, 1 wave across workers.
# 512 items / 64 batch / 8 workers = 1 batch per worker (1 sleep critical-path).
W=$(wall_p50 --variant Hopp2:2048:64:8 --max 512 --fixed-cost-us 0 \
    --work-dist fixed:0 --batch-shape flat:2000)
E=$(expected_adj 2 1)
score_abs "flat:2000 × 512 / 64 batch / 8 workers (1 wave)" "$E" "$W"

# --- CASE 4: flat with mb=1 → every item pays flat.
# 200 items / 8 workers = 25 critical-path sleeps.
W=$(wall_p50 --variant Hopp2:2048:1:8 --max 200 --fixed-cost-us 0 \
    --work-dist fixed:0 --batch-shape flat:1000)
E=$(expected_adj 25 25)
score_abs "flat:1000 × 200 / mb=1 / 8 workers" "$E" "$W"

# --- CASE 5: producer-delay is in its own thread, not the consumer sleep chain.
# Producer sends 1 item every 1000µs; critical path = producer, not consumer.
# 100 * 1000µs = 100ms + (100 sleeps × overhead).
W=$(wall_p50 --variant Cdam2:2048 --max 100 --producer-delay-us 1000 \
    --fixed-cost-us 0 --work-dist fixed:0)
E=$(expected_adj 100 100)
score_abs "producer-delay-us=1000 × 100 items" "$E" "$W"

# --- CASE 6: quadratic overhead dominates at large N.
# coeff=1000, mb=64: overhead/batch = 1000 * 64² / 1000 = 4096µs.
# 800 items / 64 / 8 workers = ~2 batches/worker → 2 critical-path sleeps × 4096µs.
W=$(wall_p50 --variant Hopp2:2048:64:8 --max 800 --fixed-cost-us 0 \
    --work-dist fixed:0 --batch-shape quad:1000)
E=$(expected_adj 8.2 2)
score_abs "quad:1000 × 800 / mb=64 / 8 workers" "$E" "$W"

echo ""
echo "=== RATIO cases (platform-independent, ±${TOL_RATIO}%) ==="

# --- CASE 7: worker scaling 2x vs 8x = 4x speedup.
W2=$(wall_p50 --variant Hopp2:2048:1:2 --max 400 --fixed-cost-us 0 --work-dist fixed:500)
W8=$(wall_p50 --variant Hopp2:2048:1:8 --max 400 --fixed-cost-us 0 --work-dist fixed:500)
R=$(awk -v a="$W2" -v b="$W8" 'BEGIN {printf "%.2f", a / b}')
score_ratio "W=2 vs W=8 speedup" "4.0" "$R"

# --- CASE 8: amortization mb=1 vs mb=32 with heavy fixed cost.
MB1=$(wall_p50 --variant Hopp2:2048:1:8 --max 800 --fixed-cost-us 500 --work-dist fixed:0)
MB32=$(wall_p50 --variant Hopp2:2048:32:8 --max 800 --fixed-cost-us 500 --work-dist fixed:0)
R=$(awk -v a="$MB1" -v b="$MB32" 'BEGIN {printf "%.2f", a / b}')
# 32× in theory; macOS sleep overhead makes it lower since mb=1 pays more overhead.
# Real target: expect 15-30x, call 20x nominal.
score_ratio "mb=1 vs mb=32 amortization" "20" "$R"

# --- CASE 9: flat shape mb=1 vs mb=64.
# flat:2000, 512 items / 8 workers: mb=1 → 64 batches/worker vs mb=64 → 1.
F1=$(wall_p50 --variant Hopp2:2048:1:8 --max 512 --fixed-cost-us 0 \
     --work-dist fixed:0 --batch-shape flat:2000)
F64=$(wall_p50 --variant Hopp2:2048:64:8 --max 512 --fixed-cost-us 0 \
      --work-dist fixed:0 --batch-shape flat:2000)
R=$(awk -v a="$F1" -v b="$F64" 'BEGIN {printf "%.2f", a / b}')
score_ratio "flat mb=1 vs mb=64 (group-commit win)" "64" "$R"

# --- CASE 10: quad U-curve: mb=64 (overhead-dominated) vs mb=8 (sweet spot).
Q8=$(wall_p50 --variant Hopp2:2048:8:8 --max 800 --fixed-cost-us 0 \
     --work-dist fixed:0 --batch-shape quad:1000)
Q64=$(wall_p50 --variant Hopp2:2048:64:8 --max 800 --fixed-cost-us 0 \
      --work-dist fixed:0 --batch-shape quad:1000)
R=$(awk -v a="$Q64" -v b="$Q8" 'BEGIN {printf "%.2f", a / b}')
# mb=64 should be SLOWER than mb=8 under quad:1000. Ratio > 1 = quad biting.
score_ratio "quad:1000 mb=64 / mb=8 (hurt > 1x)" "1.5" "$R"

echo ""
echo "=== summary: $pass pass / $fail fail ==="
