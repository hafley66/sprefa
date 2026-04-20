#!/usr/bin/env bash
# agg_probe.sh — python aggregator for probe TSV.
# Tukey-fence outliers, print p50/p95/stddev/CV% + median RSS per cell.

set -euo pipefail
TSV="${1:-target/probe_results.tsv}"

exec python3 - "$TSV" <<'PY'
import sys, csv, statistics
from collections import defaultdict

path = sys.argv[1]
cells = defaultdict(lambda: {"wall": [], "rss": [], "files": 0, "matches": 0})

with open(path) as f:
    r = csv.reader(f, delimiter='\t')
    header = next(r)
    for row in r:
        if len(row) < 8: continue
        v, p, t, files, _bytes, matches, wall, rss = row
        key = (v, p)
        cells[key]["wall"].append(float(wall))
        cells[key]["rss"].append(int(rss))
        cells[key]["files"]   = int(files)
        cells[key]["matches"] = int(matches)

def tukey(xs):
    if len(xs) < 4: return list(xs)
    s = sorted(xs)
    q1 = s[len(s)//4]
    q3 = s[(len(s)*3)//4]
    hi = q3 + 1.5 * (q3 - q1)
    return [x for x in xs if x <= hi]

def pct(xs, q):
    s = sorted(xs)
    k = max(0, min(len(s)-1, round(q * (len(s)-1))))
    return s[k]

rows = []
for (v, p), d in cells.items():
    walls = tukey(d["wall"])
    if not walls: continue
    p50 = pct(walls, 0.50)
    p95 = pct(walls, 0.95)
    mean = statistics.fmean(walls)
    sd = statistics.pstdev(walls) if len(walls) > 1 else 0.0
    cv = (sd / mean * 100) if mean else 0.0
    rmed = statistics.median(d["rss"]) / 1024 / 1024
    rmax = max(d["rss"]) / 1024 / 1024
    fps = d["files"] / (p50 / 1000) if p50 else 0
    rows.append((v, p, len(walls), len(d["wall"]), p50, p95, sd, cv, fps, rmed, rmax, d["files"], d["matches"]))

rows.sort(key=lambda r: (r[1], r[0]))

hdr = f"{'variant':<14} {'pattern':<10} {'n/N':>7} {'p50_ms':>8} {'p95_ms':>8} {'sd':>7} {'cv%':>6} {'f/s_p50':>8} {'rss_med':>8} {'rss_max':>8} {'files':>6} {'match':>6}"
print(hdr)
print("-" * len(hdr))
last_p = None
for v, p, n, N, p50, p95, sd, cv, fps, rmed, rmax, files, matches in rows:
    if last_p is not None and p != last_p:
        print()
    last_p = p
    cv_mark = "!" if cv > 10 else (" " if cv > 5 else " ")
    print(f"{v:<14} {p:<10} {n:>3}/{N:<3} {p50:>8.1f} {p95:>8.1f} {sd:>7.2f} {cv:>5.1f}{cv_mark} {fps:>8.0f} {rmed:>6.1f}MB {rmax:>6.1f}MB {files:>6} {matches:>6}")

print()
print("CV%: '!' = >10% (noisy), ' ' = <=10%")
print("n/N = kept after Tukey fence / total trials")
PY
