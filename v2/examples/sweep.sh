#!/usr/bin/env bash
# sweep.sh — matrix runner for throughput_probe.
#
# Each cell = one `throughput_probe --trials N` invocation. Probe does its
# own Monte Carlo internally and renders an ASCII bar chart on stderr. This
# script fans out over one or more knob axes, accumulates TSV rows to an
# output file, and echoes a progress chart across cells as it runs.
#
# Usage:
#   sweep.sh [opts] <name>=<v1,v2,...> [<name>=...]  [-- probe-args]
#
# Knob names:
#   variant         full variant spec, e.g. Hopp2:2048:64:8
#   pattern         baseline|medium|heavy|nomatch
#   cost            --fixed-cost-dist spec (e.g. pareto:100:15, fixed:100, exp:50)
#   work            --work-dist spec (real, fixed:N, pareto:MED:SHAPEX10)
#   workers         int — expands Hopp2:2048:64:<workers> when variant omitted
#   max_batch       int — expands Hopp2:2048:<max_batch>:<workers> (needs workers)
#   shape_x10       int — substituted into cost=pareto:MED:<shape_x10> (needs median)
#   batch           linear | flat:US | quad:COEFF — --batch-shape passthrough
#   pace            fixed:N | pareto:M:S | normal:M:S | exp:M | burst:C:G
#                   producer-side inter-send pacing distribution
#
# Options:
#   -t N            trials per cell (default 20)
#   -m N            --max files (default 10000)
#   -r PATH         --root path (default tests/smoke/.fixtures/swc)
#   -o FILE         TSV output (default target/sweep.tsv)
#   -n              dry-run; print invocations but do not execute
#   --              everything after is passed through to throughput_probe
#
# Examples:
#   sweep.sh variant=Cdam2:2048,Gopp2:2048:64,Hopp2:2048:64:8 pattern=baseline
#   sweep.sh workers=1,2,4,8,16 -t 30 -- --shuffle --fixed-cost-dist pareto:100:15
#   sweep.sh shape_x10=11,15,20,25 -t 50 -- --variant Hopp2:2048:64:8 --work-dist real

set -euo pipefail

TRIALS=20
MAX_FILES=10000
ROOT="tests/smoke/.fixtures/swc"
OUT="target/sweep.tsv"
DRY=0
PROBE_ARGS=()

while (( $# )); do
  case "$1" in
    -t) TRIALS="$2"; shift 2 ;;
    -m) MAX_FILES="$2"; shift 2 ;;
    -r) ROOT="$2"; shift 2 ;;
    -o) OUT="$2"; shift 2 ;;
    -n) DRY=1; shift ;;
    --) shift; while (( $# )); do PROBE_ARGS+=("$1"); shift; done ;;
    *=*) KV+=("$1"); shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "${KV+x}" ]]; then
  echo "sweep.sh: at least one KEY=v1,v2 required. See header." >&2
  exit 2
fi

# Build cells via cartesian product of all KV axes.
cells=("")
for axis in "${KV[@]}"; do
  key="${axis%%=*}"
  vals="${axis#*=}"
  IFS=',' read -ra vs <<<"$vals"
  new=()
  for cell in "${cells[@]}"; do
    for v in "${vs[@]}"; do
      new+=("${cell}${key}=${v};")
    done
  done
  cells=("${new[@]}")
done

# Shuffle cell order to de-correlate thermal / page-cache confound across runs.
if command -v shuf >/dev/null 2>&1; then
  mapfile -t cells < <(printf '%s\n' "${cells[@]}" | shuf)
fi

# TSV header + ensure target dir exists.
mkdir -p "$(dirname "$OUT")"
printf 'variant\tpattern\tcost_dist\twork_dist\tbatch_shape\tproducer_pace\tseed\ttrials\tfiles\tbytes\tmatches\twall_p10\twall_p50\twall_p90\twall_min\twall_max\tbatch_p50\tbatch_p95\tdisp_p50\tdisp_p95\tdisp_p99\tcell\n' > "$OUT"

# Build the probe binary once.
cd "$(dirname "$0")/.."
if (( !DRY )); then
  cargo build --release --example throughput_probe >&2
fi
BIN="target/release/examples/throughput_probe"

n_cells=${#cells[@]}
i=0
declare -a wall_p50_summary
for cell in "${cells[@]}"; do
  i=$((i + 1))
  # Parse back into assoc-array.
  declare -A kv=()
  IFS=';' read -ra kvs <<<"${cell%;}"
  for piece in "${kvs[@]}"; do
    [[ -z "$piece" ]] && continue
    kv["${piece%%=*}"]="${piece#*=}"
  done

  args=("--trials" "$TRIALS" "--max" "$MAX_FILES" "--root" "$ROOT" "--tsv")

  # Variant: either direct, or composed from workers/max_batch.
  if [[ -n "${kv[variant]:-}" ]]; then
    args+=("--variant" "${kv[variant]}")
  elif [[ -n "${kv[workers]:-}" || -n "${kv[max_batch]:-}" ]]; then
    mb="${kv[max_batch]:-64}"
    w="${kv[workers]:-8}"
    args+=("--variant" "Hopp2:2048:${mb}:${w}")
  fi

  args+=("--pattern" "${kv[pattern]:-baseline}")

  if [[ -n "${kv[cost]:-}" ]]; then
    args+=("--fixed-cost-dist" "${kv[cost]}")
  elif [[ -n "${kv[shape_x10]:-}" ]]; then
    args+=("--fixed-cost-dist" "pareto:100:${kv[shape_x10]}")
  fi

  if [[ -n "${kv[work]:-}" ]]; then
    args+=("--work-dist" "${kv[work]}")
  fi

  if [[ -n "${kv[batch]:-}" ]]; then
    args+=("--batch-shape" "${kv[batch]}")
  fi

  if [[ -n "${kv[pace]:-}" ]]; then
    if [[ "${kv[pace]}" == burst:* ]]; then
      args+=("--producer-burst" "${kv[pace]#burst:}")
    else
      args+=("--producer-pace" "${kv[pace]}")
    fi
  fi

  # Each cell gets its own seed so trial-to-trial ordering varies across cells.
  cell_seed=$(( (RANDOM << 16) | RANDOM ))
  args+=("--seed" "$cell_seed")

  args+=("${PROBE_ARGS[@]}")

  echo "[$i/$n_cells] $cell" >&2
  if (( DRY )); then
    echo "  $BIN ${args[*]}" >&2
    continue
  fi

  row="$($BIN "${args[@]}" 2> >(cat >&2))"
  echo -e "${row}\t${cell%;}" >> "$OUT"

  # Columns: 9=files 12=p10 13=p50 14=p90.
  files=$(awk -F'\t' '{print $9}' <<<"$row")
  wall_p10=$(awk -F'\t' '{print $12}' <<<"$row")
  wall_p50=$(awk -F'\t' '{print $13}' <<<"$row")
  wall_p90=$(awk -F'\t' '{print $14}' <<<"$row")
  wall_p50_summary+=("$cell|$wall_p50|$files|$wall_p10|$wall_p90")
done

echo "" >&2
echo "=== sweep summary (wall_p50 across cells) ===" >&2
# Figure the largest p50 to scale bar width.
max_p50=0
for e in "${wall_p50_summary[@]}"; do
  IFS='|' read -r _label v _files _p10 _p90 <<<"$e"
  max_p50=$(awk -v v="$v" -v m="$max_p50" 'BEGIN { print (v > m) ? v : m }')
done
cap_warned=0
noise_warned=0
for e in "${wall_p50_summary[@]}"; do
  IFS='|' read -r label v files p10 p90 <<<"$e"
  bar_len=$(awk -v v="$v" -v m="$max_p50" 'BEGIN { if (m > 0) printf "%d", (v / m) * 50; else print 0 }')
  bar=$(printf '#%.0s' $(seq 1 "$bar_len" 2>/dev/null || echo ""))
  # Spread ratio = (p90-p10)/p50. >0.10 flags a low-SNR cell.
  spread=$(awk -v a="$p10" -v b="$p90" -v m="$v" 'BEGIN { if (m > 0) printf "%.3f", (b - a) / m; else print "0" }')
  flag=""
  cap_hit=$(awk -v f="$files" -v m="$MAX_FILES" 'BEGIN { print (f < m) ? 1 : 0 }')
  if [[ "$cap_hit" == "1" ]]; then flag+=" [files=${files}<max=${MAX_FILES}]"; cap_warned=1; fi
  noisy=$(awk -v s="$spread" 'BEGIN { print (s > 0.10) ? 1 : 0 }')
  if [[ "$noisy" == "1" ]]; then flag+=" [spread=${spread}]"; noise_warned=1; fi
  printf '  %-60s %9.2f ms  |%s%s\n' "${label%;}" "$v" "$bar" "$flag" >&2
done

if (( cap_warned )); then
  echo "  WARN: one or more cells returned fewer files than requested --max; fixture exhausted." >&2
fi
if (( noise_warned )); then
  echo "  WARN: one or more cells has (p90-p10)/p50 > 0.10; raise -t or workload size." >&2
fi

echo "" >&2
echo "TSV: $OUT" >&2
