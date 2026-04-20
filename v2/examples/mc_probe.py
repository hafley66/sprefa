#!/usr/bin/env python3
"""
mc_probe.py — Monte Carlo aggregator for throughput_probe.

Reads JSONL from stdin. Each line is one run. Multiple runs may share the
same "variant" (and other config). For each group, compute across-run
percentiles (p10/p50/p90) of the specified metrics. Render ASCII bars.

Usage:
    # run same config N times with different seeds, aggregate:
    for s in 1 2 3 4 5; do
        ./throughput_probe ... --seed $s ...
    done | python3 mc_probe.py --group variant --metrics wall_ms,disp_p95_us

Options:
    --group FIELD           JSON field to group runs by (default: variant)
    --metrics F1,F2,...     metrics to aggregate
    --width N               bar width (default: 45)
    --tsv PATH              write aggregated TSV
"""
import sys, json, argparse
from collections import defaultdict

def percentile(sorted_vals, q):
    if not sorted_vals: return 0
    n = len(sorted_vals)
    idx = int(round((n - 1) * q))
    return sorted_vals[idx]

def load_rows(fp):
    rows = []
    for line in fp:
        line = line.strip()
        if not line or not line.startswith('{'): continue
        try: rows.append(json.loads(line))
        except json.JSONDecodeError: print(f"skip: {line[:80]}", file=sys.stderr)
    return rows

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--group", default="variant")
    ap.add_argument("--metrics", required=True)
    ap.add_argument("--width", type=int, default=45)
    ap.add_argument("--tsv", default=None)
    args = ap.parse_args()

    rows = load_rows(sys.stdin)
    if not rows:
        print("no rows", file=sys.stderr); sys.exit(1)

    metrics = [m.strip() for m in args.metrics.split(",")]
    groups = defaultdict(list)
    for r in rows:
        groups[r.get(args.group, "?")].append(r)

    # For each group, for each metric, compute p10/p50/p90/min/max.
    agg = {}  # group -> metric -> dict
    for g, rs in groups.items():
        agg[g] = {"n": len(rs)}
        for m in metrics:
            vals = sorted(float(r.get(m, 0) or 0) for r in rs)
            agg[g][m] = {
                "p10": percentile(vals, 0.10),
                "p50": percentile(vals, 0.50),
                "p90": percentile(vals, 0.90),
                "min": vals[0] if vals else 0,
                "max": vals[-1] if vals else 0,
                "cv_pct": (
                    100.0 * (vals[-1] - vals[0]) / vals[len(vals)//2]
                    if vals and vals[len(vals)//2] > 0 else 0
                ),
            }

    # Render: for each metric, one section showing groups as rows with
    # p10-p50-p90 overlayed as a range bar.
    for m in metrics:
        print(f"\n═══ {m} ═══")
        maxv = max(agg[g][m]["max"] for g in agg)
        if maxv == 0: continue
        label_w = max(len(str(g)) for g in agg)
        for g in sorted(agg.keys()):
            a = agg[g][m]
            n = a["p50"]
            # Bar = from p10 to p90, with a marker at p50.
            lo = int(round(args.width * a["p10"] / maxv))
            hi = int(round(args.width * a["p90"] / maxv))
            mid = int(round(args.width * a["p50"] / maxv))
            bar = [" "] * args.width
            for i in range(lo, min(hi + 1, args.width)):
                bar[i] = "─"
            if 0 <= mid < args.width:
                bar[mid] = "┼"
            if 0 <= lo < args.width:
                bar[lo] = "├"
            if 0 <= hi < args.width and hi != lo:
                if hi < args.width: bar[hi] = "┤"
            print(f"  {str(g):<{label_w}}  {''.join(bar)}  p10={a['p10']:.0f}  p50={a['p50']:.0f}  p90={a['p90']:.0f}  (n={agg[g]['n']})")
        # Raw table per metric
        print(f"  {'group':<{label_w}}    min       p10       p50       p90       max     spread%")
        for g in sorted(agg.keys()):
            a = agg[g][m]
            print(f"  {str(g):<{label_w}}  {a['min']:8.0f}  {a['p10']:8.0f}  {a['p50']:8.0f}  {a['p90']:8.0f}  {a['max']:8.0f}  {a['cv_pct']:6.1f}")

    if args.tsv:
        with open(args.tsv, "w") as f:
            cols = [args.group, "n"] + [f"{m}_{p}" for m in metrics for p in ("p10","p50","p90","min","max")]
            f.write("\t".join(cols) + "\n")
            for g in sorted(agg.keys()):
                row = [str(g), str(agg[g]["n"])]
                for m in metrics:
                    for p in ("p10","p50","p90","min","max"):
                        row.append(f"{agg[g][m][p]:.2f}")
                f.write("\t".join(row) + "\n")
        print(f"\nwrote TSV: {args.tsv}", file=sys.stderr)

if __name__ == "__main__":
    main()
