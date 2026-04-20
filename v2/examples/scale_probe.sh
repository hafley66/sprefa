#!/usr/bin/env bash
# scale_probe.sh — join 2k/10k/40k probe TSVs into a single scaling table.
# Reads:
#   target/probe_results_2k.tsv   (from saved copy)
#   target/probe_results_10k.tsv  (from saved copy)
#   target/probe_results.tsv      (current = 40k)
#
# Per (variant, pattern): p50 at each scale, f/s at each scale, RSS at each
# scale, scaling ratio (wall at larger-scale / wall at smaller-scale vs the
# expected linear ratio). Highlights sublinear / superlinear scaling.

set -euo pipefail

exec python3 - <<'PY'
import csv, statistics, os
from collections import defaultdict

scales = [
    ("2k",  "target/probe_results_2k.tsv"),
    ("10k", "target/probe_results_10k.tsv"),
    ("40k", "target/probe_results.tsv"),
]

def tukey(xs):
    if len(xs) < 4: return list(xs)
    s = sorted(xs)
    q1 = s[len(s)//4]
    q3 = s[(len(s)*3)//4]
    hi = q3 + 1.5 * (q3 - q1)
    return [x for x in xs if x <= hi]

def load(path):
    cells = defaultdict(lambda: {"wall": [], "rss": [], "files": 0})
    if not os.path.exists(path): return cells
    with open(path) as f:
        r = csv.reader(f, delimiter='\t')
        next(r, None)
        for row in r:
            if len(row) < 8: continue
            v, p, t, files, _b, _m, wall, rss = row
            cells[(v, p)]["wall"].append(float(wall))
            cells[(v, p)]["rss"].append(int(rss))
            cells[(v, p)]["files"] = int(files)
    return cells

data = {label: load(path) for label, path in scales}

keys = set()
for label, _ in scales:
    keys.update(data[label].keys())

def p50(xs):
    if not xs: return None
    s = sorted(tukey(xs))
    return s[len(s)//2]

def median(xs):
    return statistics.median(xs) if xs else None

rows = []
for v, p in sorted(keys, key=lambda k: (k[1], k[0])):
    r = {"variant": v, "pattern": p}
    for label, _ in scales:
        c = data[label].get((v, p))
        if c is None or not c["wall"]:
            r[f"{label}_ms"] = None
            r[f"{label}_fps"] = None
            r[f"{label}_rss"] = None
            r[f"{label}_n"] = c["files"] if c else 0
            continue
        ms = p50(c["wall"])
        r[f"{label}_ms"]  = ms
        r[f"{label}_n"]   = c["files"]
        r[f"{label}_fps"] = c["files"] / (ms / 1000) if ms else None
        r[f"{label}_rss"] = median(c["rss"]) / 1024 / 1024 if c["rss"] else None
    rows.append(r)

def fmt(x, w, prec=0, unit=""):
    if x is None: return " " * w + unit
    if isinstance(x, float):
        return f"{x:>{w}.{prec}f}{unit}"
    return f"{x:>{w}}{unit}"

hdr = f"{'variant':<14} {'pattern':<10}  {'2k ms':>8} {'10k ms':>8} {'40k ms':>8}   {'2k f/s':>8} {'10k f/s':>8} {'40k f/s':>8}   {'2k MB':>7} {'10k MB':>7} {'40k MB':>7}  {'scale_40/10':>11}"
print(hdr)
print("-" * len(hdr))

last_p = None
for r in rows:
    if last_p is not None and r["pattern"] != last_p: print()
    last_p = r["pattern"]
    # scaling factor 40k/10k, compared to file count ratio 40k_files / 10k_files
    scale_str = ""
    if r.get("10k_ms") and r.get("40k_ms") and r.get("10k_n") and r.get("40k_n"):
        wall_ratio = r["40k_ms"] / r["10k_ms"]
        file_ratio = r["40k_n"]  / r["10k_n"]
        eff = wall_ratio / file_ratio  # 1.0 = linear, <1 = super (better), >1 = sub (worse)
        mark = " " if abs(eff - 1.0) < 0.1 else ("↑" if eff > 1.0 else "↓")
        scale_str = f"{wall_ratio:.2f}x {mark}{eff:.2f}"
    print(
        f"{r['variant']:<14} {r['pattern']:<10} "
        f" {fmt(r.get('2k_ms'),   7, 1, 'ms'):>8}"
        f" {fmt(r.get('10k_ms'),  7, 1, 'ms'):>8}"
        f" {fmt(r.get('40k_ms'),  7, 1, 'ms'):>8}  "
        f" {fmt(r.get('2k_fps'),  8, 0):>8}"
        f" {fmt(r.get('10k_fps'), 8, 0):>8}"
        f" {fmt(r.get('40k_fps'), 8, 0):>8}  "
        f" {fmt(r.get('2k_rss'),  6, 1, 'MB'):>7}"
        f" {fmt(r.get('10k_rss'), 6, 1, 'MB'):>7}"
        f" {fmt(r.get('40k_rss'), 6, 1, 'MB'):>7} "
        f" {scale_str:>12}"
    )

print()
print("scale_40/10 = wall(40k)/wall(10k), then efficiency vs file ratio.")
print("  efficiency 1.0 = linear scaling, <1.0 super-linear (↓ better),")
print("  >1.0 sub-linear (↑ worse — parse or memory pressure).")
PY
