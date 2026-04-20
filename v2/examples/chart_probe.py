#!/usr/bin/env python3
"""
chart_probe.py — ASCII charts for throughput_probe JSONL output.

Reads one JSON object per line from stdin. Extracts (x, y) pairs from
fields specified via CLI flags. Renders horizontal bars + a numeric table.

Usage:
    ./throughput_probe ... | python3 chart_probe.py \\
        --x batch_p50 \\
        --y wall_ms,disp_p50_us,disp_p95_us,disp_max_us \\
        --title "Pareto B-sweep"

Options:
    --x FIELD            x-axis field (label column)
    --y F1,F2,F3         comma-separated y-axis fields
    --title TEXT         chart title
    --width N            bar width in chars (default: 60)
    --tsv PATH           also write raw data as TSV
    --log                log-scale the y values (base 10)
"""
import sys, json, argparse, math

def load_rows(fp):
    rows = []
    for line in fp:
        line = line.strip()
        if not line or not line.startswith('{'):
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            print(f"skip non-json: {line[:80]}", file=sys.stderr)
    return rows

def fmt_num(v):
    if isinstance(v, float):
        return f"{v:.1f}"
    return str(v)

def bar(val, vmax, width, log=False):
    if vmax == 0: return ""
    if log:
        v = math.log10(max(1, val))
        m = math.log10(max(1, vmax))
        frac = v / m if m > 0 else 0
    else:
        frac = val / vmax if vmax > 0 else 0
    n = int(round(frac * width))
    return "█" * n

def render_series(rows, xfield, yfield, width, log, label_col_w):
    vmax = max((r.get(yfield, 0) or 0) for r in rows)
    if vmax == 0:
        print(f"  (no data for {yfield})")
        return
    print(f"  {yfield}  (max={fmt_num(vmax)}{', log10 scale' if log else ''})")
    for r in rows:
        x = r.get(xfield, "?")
        y = r.get(yfield, 0) or 0
        b = bar(y, vmax, width, log)
        print(f"    {str(x):<{label_col_w}} │ {b:<{width}} {fmt_num(y)}")
    print()

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--x", required=True, help="x-axis field name")
    ap.add_argument("--y", required=True, help="comma-separated y fields")
    ap.add_argument("--title", default="")
    ap.add_argument("--width", type=int, default=60)
    ap.add_argument("--tsv", default=None)
    ap.add_argument("--log", action="store_true")
    args = ap.parse_args()

    rows = load_rows(sys.stdin)
    if not rows:
        print("no rows", file=sys.stderr)
        sys.exit(1)

    yfields = [f.strip() for f in args.y.split(",")]
    label_w = max(len(str(r.get(args.x, "?"))) for r in rows)

    if args.title:
        print(f"═══ {args.title} ═══\n")

    for yf in yfields:
        render_series(rows, args.x, yf, args.width, args.log, label_w)

    # Numeric table
    cols = [args.x] + yfields
    widths = {c: max(len(c), max(len(fmt_num(r.get(c, ""))) for r in rows)) for c in cols}
    header = " │ ".join(f"{c:>{widths[c]}}" for c in cols)
    print(header)
    print("─" * len(header))
    for r in rows:
        line = " │ ".join(f"{fmt_num(r.get(c, '')):>{widths[c]}}" for c in cols)
        print(line)

    if args.tsv:
        with open(args.tsv, "w") as f:
            f.write("\t".join(cols) + "\n")
            for r in rows:
                f.write("\t".join(str(r.get(c, "")) for c in cols) + "\n")
        print(f"\nwrote TSV: {args.tsv}", file=sys.stderr)

if __name__ == "__main__":
    main()
