#!/usr/bin/env python3
"""Compare two normal-form edge tsvs (src_path src_name dst_path dst_name).
Prints |a|, |b|, |a intersect b|, and a 20-row sample of each difference set."""
import sys


def load(path):
    rows = set()
    with open(path) as fh:
        for line in fh:
            line = line.rstrip("\n")
            if not line:
                continue
            rows.add(line)
    return rows


def sample(rows, n=20):
    return sorted(rows)[:n]


def main():
    if len(sys.argv) != 3:
        print("usage: bench.py <a.tsv> <b.tsv>", file=sys.stderr)
        sys.exit(2)
    a_path, b_path = sys.argv[1], sys.argv[2]
    a = load(a_path)
    b = load(b_path)
    inter = a & b
    a_only = a - b
    b_only = b - a

    print(f"a: {a_path}")
    print(f"b: {b_path}")
    print(f"|a| = {len(a)}")
    print(f"|b| = {len(b)}")
    print(f"|a ∩ b| = {len(inter)}")
    print(f"|a - b| = {len(a_only)}")
    print(f"|b - a| = {len(b_only)}")
    if a:
        print(f"recall (a∩b / a) = {len(inter) / len(a):.4f}")
    if b:
        print(f"precision (a∩b / b) = {len(inter) / len(b):.4f}")

    print(f"\n-- {len(a_only)} rows only in a, first 20 --")
    for row in sample(a_only):
        print(row)

    print(f"\n-- {len(b_only)} rows only in b, first 20 --")
    for row in sample(b_only):
        print(row)


if __name__ == "__main__":
    main()
