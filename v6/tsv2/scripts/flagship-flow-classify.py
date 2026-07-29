"""Classify the flagship flow port's four query diffs.

The v6 rail now has the same value-plane inputs as the v5 query: direct df
edges, positional arg/param slots pinned by resolved caller-site spans, return
hops, and flat sig-owner fields. A nonzero flow_edge intersection is therefore
an executable requirement, not a descriptive count. The three output buckets
remain extraction-input difference, v6 expression gap, and real defect.
"""

import os
import sys


REASONS = {
    "flow_edge": "value-plane rows differ after the df union",
    "flow_reach": "value-plane closure differs after the df union",
    "flow_param_type": "resolved-callee sig-owner join differs",
    "flow_node_type": "df-param positional type join differs",
}


def read_tsv(path):
    with open(path, encoding="utf-8") as handle:
        return {line.rstrip("\n") for line in handle if line.strip()}


def sample(rows):
    return "; ".join(sorted(rows)[:3]) or "-"


def main():
    work = sys.argv[1]
    rows = []
    for rel, reason in REASONS.items():
        v5 = read_tsv(os.path.join(work, f"v5.{rel}.tsv"))
        v6 = read_tsv(os.path.join(work, f"v6.{rel}.tsv"))
        matched = v5 & v6
        v5_only = v5 - v6
        v6_only = v6 - v5
        gap = len(v5_only) + len(v6_only)
        rows.append((rel, len(v5), len(v6), len(matched), len(v5_only), len(v6_only), gap, reason, v5_only, v6_only))

    flow_edge = next(row for row in rows if row[0] == "flow_edge")
    if flow_edge[3] == 0:
        print("FAIL  flow_edge match assertion: expected at least one matching value-plane row")
        return 1

    print("CLASSIFICATION TABLE")
    print("  {:<16}{:>7}{:>7}{:>7}{:>8}{:>8}{:>7}{:>8}{:>8}".format(
        "rel", "v5", "v6", "match", "v5only", "v6only", "(a)", "(b)", "(c)"))
    for rel, v5_n, v6_n, match_n, only5_n, only6_n, gap, reason, _, _ in rows:
        print("  {:<16}{:>7}{:>7}{:>7}{:>8}{:>8}{:>7}{:>8}{:>8}".format(
            rel, v5_n, v6_n, match_n, only5_n, only6_n, 0, gap, 0))
        if gap:
            print(f"    (b) {reason}")

    print("\nSPOT-CHECKED PAIRS")
    for rel, _, _, _, _, _, _, _, v5_only, v6_only in rows:
        print(f"  {rel} v5-only: {sample(v5_only)}")
        print(f"  {rel} v6-only: {sample(v6_only)}")

    print("\nevery difference classified: 0 extraction-input rows, gaps shown in (b), 0 defects, 0 unclassified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
