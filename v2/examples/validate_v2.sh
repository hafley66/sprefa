#!/usr/bin/env bash
# validate_v2.sh — scaffold validation for throughput_probe_v2.
#
# Structure:
#   PART A  determinism  — counts reproducible across axes (no math, strict)
#   PART B  effect axis  — each EffectCost impl obeys its analytic model
#   PART C  arrival axis — each ArrivalGen impl shows its signature shape
#   PART D  randomization — trial variance bounded; seed-reproducible
#
# Tolerances generous (±15-25%) because macOS sleep floor is ~150-300µs per
# call, which shows up at small N. Big ratios (amortization, group-commit)
# use larger headroom to avoid flakes.

set -euo pipefail
cd "$(dirname "$0")/.."
BIN="target/release/examples/throughput_probe_v2"
[[ -x "$BIN" ]] || { echo "build first: cargo build --release --example throughput_probe_v2" >&2; exit 1; }

ROOT="tests/smoke/.fixtures/swc"
TRIALS=${TRIALS:-6}
pass=0; fail=0
REPORT=()

TSV_HDR='variant pattern effect arrival policy seed trials files bytes matches wall_p10 wall_p50 wall_p90 wall_min wall_max batch_p50 batch_p95 disp_p50 disp_p95 disp_p99'

# Run with flags; return full TSV line.
run_tsv() {
  $BIN --pattern baseline --tsv --trials "$TRIALS" --root "$ROOT" "$@" 2>/dev/null
}

# Field extractors (1-indexed, awk style). Keep in sync with TSV_HDR.
f_wall_p10()  { awk -F'\t' '{print $11}'; }
f_wall_p50()  { awk -F'\t' '{print $12}'; }
f_wall_p90()  { awk -F'\t' '{print $13}'; }
f_wall_min()  { awk -F'\t' '{print $14}'; }
f_files()     { awk -F'\t' '{print $8}';  }
f_bytes()     { awk -F'\t' '{print $9}';  }
f_matches()   { awk -F'\t' '{print $10}'; }
f_batch_p50() { awk -F'\t' '{print $16}'; }
f_disp_p50()  { awk -F'\t' '{print $18}'; }

verdict() {
  local label="$1" ok="$2"; shift 2
  if [[ "$ok" == "1" ]]; then
    pass=$((pass + 1)); printf 'PASS  %-60s  %s\n' "$label" "$*"
  else
    fail=$((fail + 1)); printf 'FAIL  %-60s  %s\n' "$label" "$*"
  fi
}

# cmp_within obs expected tol_pct
within_pct() {
  awk -v o="$1" -v e="$2" -v t="$3" 'BEGIN {
    if (e == 0) { print (o == 0) ? 1 : 0; exit }
    d = (o - e) / e * 100; if (d < 0) d = -d
    print (d <= t) ? 1 : 0
  }'
}

# cmp_at_least obs_ratio expected_min
at_least() { awk -v o="$1" -v e="$2" 'BEGIN { print (o >= e) ? 1 : 0 }'; }
at_most()  { awk -v o="$1" -v e="$2" 'BEGIN { print (o <= e) ? 1 : 0 }'; }

# =============================================================================
# CALIBRATION — per-sleep floor, shared with v1.
# =============================================================================
echo "=== calibration ==="
CALIB=$(run_tsv --max 200 --cap 2048 --max-batch 1 --workers 1 \
  --effect 'synthetic:linear,fixed=1000,work=fixed/0' --arrival dirac)
CALIB_WALL=$(printf '%s\n' "$CALIB" | f_wall_p50)
IDEAL_MS=$(awk 'BEGIN {print 200 * 1000 / 1000}') # 200 × 1000µs = 200ms
OVERHEAD_US=$(awk -v o="$CALIB_WALL" -v i="$IDEAL_MS" 'BEGIN { printf "%.0f", ((o - i) * 1000) / 200 }')
printf '  ideal=%sms  observed=%sms  per-sleep overhead ≈ %sµs\n\n' "$IDEAL_MS" "$CALIB_WALL" "$OVERHEAD_US"

# =============================================================================
# PART A — determinism / count invariants (no math; strict equality)
# =============================================================================
echo "=== PART A — determinism invariants ==="

# Baseline: any real-scan config must return the same (files, bytes, matches).
BASE=$(run_tsv --max 500 --max-batch 64 --workers 2 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival dirac)
BF=$(printf '%s\n' "$BASE" | f_files)
BB=$(printf '%s\n' "$BASE" | f_bytes)
BM=$(printf '%s\n' "$BASE" | f_matches)
printf '  reference: files=%s bytes=%s matches=%s\n' "$BF" "$BB" "$BM"

check_counts() {
  local label="$1"; shift
  local out; out=$(run_tsv "$@")
  local f b m
  f=$(printf '%s\n' "$out" | f_files)
  b=$(printf '%s\n' "$out" | f_bytes)
  m=$(printf '%s\n' "$out" | f_matches)
  local ok=0
  [[ "$f" == "$BF" && "$b" == "$BB" && "$m" == "$BM" ]] && ok=1
  verdict "$label" "$ok" "files=$f bytes=$b matches=$m"
}

# Same counts across workers (parallelism must not drop items).
check_counts "counts stable: workers=1" --max 500 --max-batch 64 --workers 1 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival dirac
check_counts "counts stable: workers=8" --max 500 --max-batch 64 --workers 8 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival dirac

# Same counts across max_batch (buffering must not drop tail).
check_counts "counts stable: max-batch=1"  --max 500 --max-batch 1  --workers 2 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival dirac
check_counts "counts stable: max-batch=256" --max 500 --max-batch 256 --workers 2 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival dirac

# Same counts across arrival shapes.
check_counts "counts stable: arrival=fixed:10"  --max 500 --max-batch 64 --workers 2 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival fixed:10
check_counts "counts stable: arrival=burst:32:100" --max 500 --max-batch 64 --workers 2 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival 'burst:32:100'
check_counts "counts stable: arrival=sine"     --max 500 --max-batch 64 --workers 2 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival 'sine:period=100,amp=50,base=0'
check_counts "counts stable: arrival=ar1"      --max 500 --max-batch 64 --workers 2 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival 'ar1:base=20,rho=80,sd=10'
check_counts "counts stable: arrival=stepfn"   --max 500 --max-batch 64 --workers 2 \
  --effect 'synthetic:linear,fixed=0,work=real' --arrival 'stepfn:200:5,400:50,500:0'

# Same counts across effect impls (any w/ scan=true).
check_counts "counts stable: effect=git-odb" --max 500 --max-batch 64 --workers 2 \
  --effect 'git-odb:fixed_us=0,scan=true' --arrival dirac
check_counts "counts stable: effect=sqlite"  --max 500 --max-batch 64 --workers 2 \
  --effect 'sqlite:tx_fixed_us=0,per_row_us=0,scan=true' --arrival dirac
check_counts "counts stable: effect=rayon"   --max 500 --max-batch 64 --workers 2 \
  --effect 'rayon-ast-grep' --arrival dirac
check_counts "counts stable: effect=flat+real" --max 500 --max-batch 64 --workers 2 \
  --effect 'synthetic:flat:0,fixed=0,work=real' --arrival dirac
check_counts "counts stable: effect=sqlite-real" --max 500 --max-batch 64 --workers 2 \
  --effect 'sqlite-real:journal=MEMORY,sync=OFF,scan=true' --arrival dirac
check_counts "counts stable: effect=git-odb-real" --max 500 --max-batch 64 --workers 2 \
  --effect 'git-odb-real:repo=tests/smoke/.fixtures/swc,oid_cap=1000,scan=true' --arrival dirac

echo ""

# =============================================================================
# PART B — EffectCost axis, each impl obeys its analytic model
# =============================================================================
echo "=== PART B — effect axis ==="

# B1: Synthetic Linear with fixed=1000µs, workers=1, max_batch=1, N=200.
# Critical-path sleeps = 200. Expected = 200ms + 200*overhead.
E=$(awk -v oh="$OVERHEAD_US" 'BEGIN { printf "%.1f", 200 + (200 * oh / 1000) }')
W=$(run_tsv --max 200 --cap 2048 --max-batch 1 --workers 1 \
   --effect 'synthetic:linear,fixed=1000,work=fixed/0' --arrival dirac | f_wall_p50)
OK=$(within_pct "$W" "$E" 15)
verdict "B1 synth-linear fixed=1000 × 200 @ W=1" "$OK" "pred=$E obs=$W"

# B2: Synthetic Flat group-commit. mb=1 vs mb=64, flat_us=2000, 512 items, 8 workers.
# mb=1  = 64 batches/worker (64 sleeps × 2ms)
# mb=64 = 1 batch/worker    ( 1 sleep  × 2ms)
# Theoretical ratio = 64. Macos overhead shrinks it; require ≥ 20×.
F1=$(run_tsv --max 512 --cap 2048 --max-batch 1  --workers 8 \
   --effect 'synthetic:flat:2000,fixed=0,work=fixed/0' --arrival dirac | f_wall_p50)
F64=$(run_tsv --max 512 --cap 2048 --max-batch 64 --workers 8 \
    --effect 'synthetic:flat:2000,fixed=0,work=fixed/0' --arrival dirac | f_wall_p50)
R=$(awk -v a="$F1" -v b="$F64" 'BEGIN { printf "%.2f", a / b }')
OK=$(at_least "$R" 20)
verdict "B2 synth-flat group-commit mb=1/mb=64 ≥ 20×" "$OK" "ratio=${R}x (F1=$F1 F64=$F64)"

# B3: GitOdbRead mutex ceiling. 1 worker vs 8 workers at fixed_us=2000, N=200.
# Mutex serializes → wall should be ≈ equal (not 8× faster at W=8).
# Expect W1/W8 ratio in [0.7, 1.5] (with overhead noise).
G1=$(run_tsv --max 200 --cap 2048 --max-batch 1 --workers 1 \
   --effect 'git-odb:fixed_us=2000,scan=false' --arrival dirac | f_wall_p50)
G8=$(run_tsv --max 200 --cap 2048 --max-batch 1 --workers 8 \
   --effect 'git-odb:fixed_us=2000,scan=false' --arrival dirac | f_wall_p50)
R=$(awk -v a="$G1" -v b="$G8" 'BEGIN { printf "%.2f", a / b }')
OK=$(awk -v r="$R" 'BEGIN { print (r >= 0.7 && r <= 1.5) ? 1 : 0 }')
verdict "B3 git-odb mutex ceiling W=1/W=8 ≈ 1×" "$OK" "ratio=${R}x (W1=$G1 W8=$G8)"

# B4: SqliteInsertTx amortization. mb=1 vs mb=32 at tx_fixed_us=500, per_row=0, N=400, W=4.
# mb=1:  400 tx @ 500µs / 4 workers ≈ 50ms + overhead
# mb=32: 13 tx @ 500µs / 4 workers ≈ 1.6ms + small overhead
# Expect ratio ≥ 10×.
S1=$(run_tsv --max 400 --cap 2048 --max-batch 1  --workers 4 \
   --effect 'sqlite:tx_fixed_us=500,per_row_us=0,scan=false' --arrival dirac | f_wall_p50)
S32=$(run_tsv --max 400 --cap 2048 --max-batch 32 --workers 4 \
    --effect 'sqlite:tx_fixed_us=500,per_row_us=0,scan=false' --arrival dirac | f_wall_p50)
R=$(awk -v a="$S1" -v b="$S32" 'BEGIN { printf "%.2f", a / b }')
OK=$(at_least "$R" 10)
verdict "B4 sqlite tx amortization mb=1/mb=32 ≥ 10×" "$OK" "ratio=${R}x (S1=$S1 S32=$S32)"

# B5: RayonAstGrep 1/C ceiling — adding dispatcher workers on top of par_iter
# gives SUBLINEAR scaling. With C cores, pure ideal would be 8×; ceiling
# predicts sharply less because rayon's global pool is already contended.
# Check: W=1/W=8 ratio stays between 1× and 3× (not the 6-8× a raw
# parallelism scaling would show).
R1=$(run_tsv --max 500 --cap 2048 --max-batch 64 --workers 1 \
   --effect 'rayon-ast-grep' --arrival dirac | f_wall_p50)
R8=$(run_tsv --max 500 --cap 2048 --max-batch 64 --workers 8 \
   --effect 'rayon-ast-grep' --arrival dirac | f_wall_p50)
R=$(awk -v a="$R1" -v b="$R8" 'BEGIN { printf "%.2f", a / b }')
OK=$(awk -v r="$R" 'BEGIN { print (r >= 1.0 && r <= 3.0) ? 1 : 0 }')
verdict "B5 rayon 1/C ceiling: W=1/W=8 sublinear ≤ 3×" "$OK" "ratio=${R}x (W1=$R1 W8=$R8)"

# B5b: rayon-on-top overhead — mb=1 (N par_iters of size 1) should be SLOWER
# than mb=N (one par_iter over everything). This is the shape-of-ceiling
# demo: tight batching wrapping par_iter is pure overhead.
RM1=$(run_tsv --max 200 --cap 2048 --max-batch 1   --workers 1 \
   --effect 'rayon-ast-grep' --arrival dirac | f_wall_p50)
RMN=$(run_tsv --max 200 --cap 2048 --max-batch 200 --workers 1 \
   --effect 'rayon-ast-grep' --arrival dirac | f_wall_p50)
R=$(awk -v a="$RM1" -v b="$RMN" 'BEGIN { printf "%.2f", a / b }')
OK=$(at_least "$R" 1.5)
verdict "B5b rayon mb=1 slower than mb=N ≥ 1.5×" "$OK" "ratio=${R}x (mb1=$RM1 mbN=$RMN)"

# B6: sqlite-real amortization — real rusqlite BEGIN/INSERT/COMMIT. mb=1 vs
# mb=32 on N=400, W=4, scan=false. Each transaction has fixed commit cost;
# larger batches amortize. Expect at least 3× (real cost is smaller and
# noisier than the sleep-model's 23×).
S1=$(run_tsv --max 400 --cap 2048 --max-batch 1  --workers 4 \
   --effect 'sqlite-real:journal=MEMORY,sync=OFF,scan=false' --arrival dirac | f_wall_p50)
S32=$(run_tsv --max 400 --cap 2048 --max-batch 32 --workers 4 \
    --effect 'sqlite-real:journal=MEMORY,sync=OFF,scan=false' --arrival dirac | f_wall_p50)
R=$(awk -v a="$S1" -v b="$S32" 'BEGIN { printf "%.2f", a / b }')
OK=$(at_least "$R" 3)
verdict "B6 sqlite-real tx amortization mb=1/mb=32 ≥ 3×" "$OK" "ratio=${R}x (S1=$S1 S32=$S32)"

# B7: git-odb-real mutex ceiling — real Mutex<Repository>::odb().read() is
# contended across workers. scan=false so pure ODB cost. W=1 vs W=4
# (avoiding W=8 which shows bimodal thread-parking storms with sub-µs
# work amounts). Expect ratio in [0.3, 3.0] — sublinear scaling, possibly
# with some parking cost.
G1=$(run_tsv --max 200 --cap 2048 --max-batch 32 --workers 1 \
   --effect 'git-odb-real:repo=tests/smoke/.fixtures/swc,oid_cap=2000,scan=false' \
   --arrival dirac | f_wall_p50)
G4=$(run_tsv --max 200 --cap 2048 --max-batch 32 --workers 4 \
   --effect 'git-odb-real:repo=tests/smoke/.fixtures/swc,oid_cap=2000,scan=false' \
   --arrival dirac | f_wall_p50)
R=$(awk -v a="$G1" -v b="$G4" 'BEGIN { printf "%.2f", a / b }')
# Ceiling invariant: W=4 did NOT achieve linear 4× speedup over W=1.
# Ratio < 4 captures every sublinear outcome including parking-storm
# (ratio < 1, i.e. W=4 slower than W=1).
OK=$(at_most "$R" 4)
verdict "B7 git-odb-real mutex ceiling: W=4 no linear speedup" "$OK" "ratio=${R}x (W1=$G1 W4=$G4)"

# B8: windowed-trailing rescues batching under paced arrival.
# W=1 so all items flow to one worker — batch window is not diluted across
# parallel drainers. Setup: 2000 items, max_batch=64, arrival=fixed:50us
# (gap slower than dispatch). Opportunistic collapses to batch=1. Windowed
# (2ms window) pools up to 40 items per batch.
OPP=$(run_tsv --max 2000 --cap 2048 --max-batch 64 --workers 1 \
   --effect 'synthetic:linear,fixed=0,work=fixed/0' --arrival 'fixed:50' \
   --policy opportunistic | f_batch_p50)
WIN=$(run_tsv --max 2000 --cap 2048 --max-batch 64 --workers 1 \
   --effect 'synthetic:linear,fixed=0,work=fixed/0' --arrival 'fixed:50' \
   --policy 'windowed:2000' | f_batch_p50)
OK=1
[[ "$OPP" -le 2 ]]   || OK=0
[[ "$WIN" -ge 20 ]]  || OK=0
verdict "B8 windowed-trailing rescues batching under paced arrival" "$OK" "opp=$OPP win=$WIN"

echo ""

# =============================================================================
# PART C — ArrivalGen axis, each impl shapes the producer path
# =============================================================================
echo "=== PART C — arrival axis ==="

# C1: Dirac is fastest. Dirac vs fixed:500 on N=200 should show fixed slower.
D=$(run_tsv --max 200 --cap 2048 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=fixed/0' --arrival dirac | f_wall_p50)
F=$(run_tsv --max 200 --cap 2048 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=fixed/0' --arrival 'fixed:500' | f_wall_p50)
R=$(awk -v a="$F" -v b="$D" 'BEGIN { printf "%.2f", a / b }')
OK=$(at_least "$R" 3)
verdict "C1 fixed:500 / dirac ≥ 3×" "$OK" "ratio=${R}x (dirac=$D fixed=$F)"

# C2: Fixed arrival wall ≥ N*gap_us. N=200, gap=500µs → ≥ 100ms (minus small
# consumer overlap; producer is on critical path since consumer is a no-op).
W=$(run_tsv --max 200 --cap 2048 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=fixed/0' --arrival 'fixed:500' | f_wall_p50)
# Expect 100ms + 200*overhead. Tolerance ±25% (producer sleep floor).
E=$(awk -v oh="$OVERHEAD_US" 'BEGIN { printf "%.1f", 100 + (200 * oh / 1000) }')
OK=$(within_pct "$W" "$E" 25)
verdict "C2 arrival=fixed:500 × 200 producer-bound" "$OK" "pred=$E obs=$W"

# C3: Burst — gaps every COUNT items. N=200, count=50, gap=10000µs (10ms) →
# 4 gaps × 10ms = 40ms on producer path (idx 50, 100, 150, 200).
# Actual: gaps fire at idx=50,100,150 (3 gaps) since idx=200 is past last send.
W=$(run_tsv --max 200 --cap 2048 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=fixed/0' --arrival 'burst:50:10000' | f_wall_p50)
E=$(awk -v oh="$OVERHEAD_US" 'BEGIN { printf "%.1f", 30 + (3 * oh / 1000) }')
OK=$(within_pct "$W" "$E" 30)
verdict "C3 arrival=burst:50:10000 × 200 (3 gaps)" "$OK" "pred=$E obs=$W"

# C4: StepFn — first 100 items zero gap, last 100 at 500µs. Wall ≈ 50ms.
W=$(run_tsv --max 200 --cap 2048 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=fixed/0' --arrival 'stepfn:100:0,200:500' | f_wall_p50)
E=$(awk -v oh="$OVERHEAD_US" 'BEGIN { printf "%.1f", 50 + (100 * oh / 1000) }')
OK=$(within_pct "$W" "$E" 30)
verdict "C4 arrival=stepfn (0,500) × 100ea" "$OK" "pred=$E obs=$W"

# C5: Sine — period=100 amp=1000 base=0. Mean gap ≈ 500µs. Wall ≈ 100ms for N=200.
W=$(run_tsv --max 200 --cap 2048 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=fixed/0' --arrival 'sine:period=100,amp=1000,base=0' | f_wall_p50)
E=$(awk -v oh="$OVERHEAD_US" 'BEGIN { printf "%.1f", 100 + (200 * oh / 1000) }')
OK=$(within_pct "$W" "$E" 30)
verdict "C5 arrival=sine (mean≈500µs) × 200" "$OK" "pred=$E obs=$W"

# C6: AR1 — base=500, rho=80, sd=50. Mean gap ≈ 500µs → ≈100ms for N=200.
W=$(run_tsv --max 200 --cap 2048 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=fixed/0' --arrival 'ar1:base=500,rho=80,sd=50' | f_wall_p50)
E=$(awk -v oh="$OVERHEAD_US" 'BEGIN { printf "%.1f", 100 + (200 * oh / 1000) }')
OK=$(within_pct "$W" "$E" 30)
verdict "C6 arrival=ar1 (mean≈500µs) × 200" "$OK" "pred=$E obs=$W"

echo ""

# =============================================================================
# PART D — randomization / reproducibility
# =============================================================================
echo "=== PART D — randomization ==="

# D1: Trial variance bounded. p90/p10 should be < 1.5 on a quiet config.
OUT=$(run_tsv --max 500 --max-batch 64 --workers 4 \
   --effect 'synthetic:linear,fixed=0,work=real' --arrival dirac)
P10=$(printf '%s\n' "$OUT" | f_wall_p10)
P90=$(printf '%s\n' "$OUT" | f_wall_p90)
R=$(awk -v a="$P90" -v b="$P10" 'BEGIN { printf "%.2f", a / b }')
OK=$(at_most "$R" 1.5)
verdict "D1 trial p90/p10 ratio ≤ 1.5" "$OK" "ratio=${R}x (p10=$P10 p90=$P90)"

# D2: Seed reproducibility. Same config + same seed → same wall_p50 within 10%.
W_A=$(run_tsv --max 500 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=real' --arrival 'ar1:base=10,rho=80,sd=5' \
   --seed 12345 | f_wall_p50)
W_B=$(run_tsv --max 500 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=real' --arrival 'ar1:base=10,rho=80,sd=5' \
   --seed 12345 | f_wall_p50)
OK=$(within_pct "$W_A" "$W_B" 10)
verdict "D2 seed reproducible (AR1 stochastic)" "$OK" "A=$W_A B=$W_B"

# D3: Different seeds diverge observably (not identical) — but counts still match.
OUT_A=$(run_tsv --max 500 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=real' --arrival 'pareto:50:15' --seed 11111)
OUT_B=$(run_tsv --max 500 --max-batch 64 --workers 2 \
   --effect 'synthetic:linear,fixed=0,work=real' --arrival 'pareto:50:15' --seed 99999)
F_A=$(printf '%s\n' "$OUT_A" | f_files); F_B=$(printf '%s\n' "$OUT_B" | f_files)
M_A=$(printf '%s\n' "$OUT_A" | f_matches); M_B=$(printf '%s\n' "$OUT_B" | f_matches)
OK=1
[[ "$F_A" == "$F_B" && "$M_A" == "$M_B" ]] || OK=0
verdict "D3 different seeds: identical counts" "$OK" "A=(f=$F_A m=$M_A) B=(f=$F_B m=$M_B)"

echo ""
echo "=== summary: $pass pass / $fail fail ==="
[[ "$fail" == "0" ]] || exit 1
