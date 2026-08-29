#!/usr/bin/env python3
"""The agreed-and-missed set: (codeql2 INTERSECT vta) MINUS ours.

Two independent tools name the same edge and we do not, so the row is a gap in
our resolve legs rather than an oracle-scope artifact. Prints the four counts
plus recall/precision in the report convention (recall = overlap / |oracle|,
precision = overlap / |ours|), and writes the missed set when --out is given.
"""
import argparse


def load(path):
    with open(path) as fh:
        return {line.rstrip("\n") for line in fh if line.strip()}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ours")
    ap.add_argument("vta")
    ap.add_argument("codeql")
    ap.add_argument("--out")
    args = ap.parse_args()

    ours = load(args.ours)
    vta = load(args.vta)
    codeql = load(args.codeql)

    agreed = codeql & vta
    missed = agreed - ours

    print(f"|ours|   = {len(ours)}")
    print(f"|vta|    = {len(vta)}")
    print(f"|codeql| = {len(codeql)}")
    print(f"ours vs vta:    overlap {len(ours & vta)} "
          f"recall {len(ours & vta) / len(vta):.4f} "
          f"precision {len(ours & vta) / len(ours):.4f}")
    print(f"codeql vs vta:  overlap {len(codeql & vta)} "
          f"recall {len(codeql & vta) / len(vta):.4f} "
          f"precision {len(codeql & vta) / len(codeql):.4f}")
    print(f"ours vs codeql: overlap {len(ours & codeql)} "
          f"recall {len(ours & codeql) / len(codeql):.4f} "
          f"precision {len(ours & codeql) / len(ours):.4f}")
    print(f"agreed (codeql AND vta) = {len(agreed)}")
    print(f"agreed and missed       = {len(missed)}")

    if args.out:
        with open(args.out, "w") as fh:
            fh.write("\n".join(sorted(missed)) + "\n")


if __name__ == "__main__":
    main()
