#!/usr/bin/env bash
# Render charts from results.csv with gnuplot. One PNG per metric; a line per
# engine, x = node count (log). WALL rows (aborts) are dropped from the numeric
# plots. Pure gnuplot, no plotting library.
set -uo pipefail
CSV="$1"; OUT="$2"

# Split the CSV into one data file per engine so gnuplot can draw a line each.
engines=$(tail -n +2 "$CSV" | cut -d, -f1 | sort -u)
datadir="$OUT/data"; mkdir -p "$datadir"
for e in $engines; do
  # cols: nodes retract_ms setup_ms rss_mb ops  (skip WALL rows = non-numeric)
  awk -F, -v e="$e" '$1==e && $6!="WALL" && $6!="" {print $2, $6, $5, $8, $7}' "$CSV" \
    | sort -n > "$datadir/$e.dat"
done

plot_metric() { # title ycol ylabel outfile logy
  local title="$1" ycol="$2" ylabel="$3" outfile="$4" logy="$5"
  local plots="" first=1
  for e in $engines; do
    [[ -s "$datadir/$e.dat" ]] || continue
    [[ $first == 1 ]] && { plots="plot"; first=0; } || plots="$plots,"
    plots="$plots '$datadir/$e.dat' using 1:$ycol with linespoints title '$e'"
  done
  [[ -z "$plots" ]] && return
  gnuplot <<EOF
set terminal pngcairo size 900,560 font 'Helvetica,12'
set output '$OUT/$outfile'
set title "$title"
set xlabel 'nodes'
set ylabel '$ylabel'
set logscale x 10
$( [[ "$logy" == log ]] && echo 'set logscale y 10' )
set grid
set key top left
set datafile missing "WALL"
$plots
EOF
  echo "chart -> $OUT/$outfile"
}

plot_metric "Retract latency (the measured incremental op)" 2 "retract ms" "retract_ms.png" log
plot_metric "Setup latency (one-time build)"               3 "setup ms"   "setup_ms.png"   log
plot_metric "Peak RSS"                                     4 "peak RSS MB" "rss_mb.png"     lin
plot_metric "Incremental work (ops per retract)"          5 "ops"        "ops.png"        log
