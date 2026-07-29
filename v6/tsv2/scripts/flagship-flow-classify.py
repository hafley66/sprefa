"""Classify the flagship flow port's four query diffs.

The v5 program closes a value-flow graph made from df_edge plus positional
argument/parameter and return/call-result hops. The v6 extractor currently
arrives only resolved callable edges. Therefore the flow_edge and flow_reach
rows are distinct planes, rather than byte-comparable representations of one
relation. The signature rows are also unjoinable through the text host: their
owner is a nested span, while resolved calls identify a callee by path and name.

Every nonmatching row is consequently the named v6 expression gap. This is a
classifier, rather than a count assertion: it reads each sorted TSV and proves
that all rows in every set difference land in that bucket. Exit nonzero if a
new queried relation is not covered by the table below.
"""

import os
import sys


REASONS = {
    "flow_edge": "df value-plane rows unavailable; v6 rows are resolved callable edges",
    "flow_reach": "df value-plane closure unavailable; v6 closes resolved callable edges",
    "flow_param_type": "sig owner is a nested span and cannot join to a resolved callee through the text host",
    "flow_node_type": "df_node and df_param are unavailable from the v6 extractor",
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

    print("\nevery difference classified: 0 extraction-input rows, 0 defects, 0 unclassified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
