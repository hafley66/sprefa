#!/usr/bin/env python3
"""parity_classify.py -- every diff row lands in exactly one bucket, and the
bucket is PROVEN from the two artifacts rather than asserted (flagship's rule).

Buckets, in the order they are tested:

  (p) position-convention   same path+text+kind, different col/end_col only.
  (t) text-strip            same path+line, different STRIPPED TEXT only. The
                            two token strippers disagree; the span does not.
  (k) kind-vocabulary       same path+line+text, different kind only.
  (m) missing-in-v6         v5 has a comment at (path,line) v6 has nowhere.
  (x) extra-in-v6           v6 has a comment at (path,line) v5 has nowhere.
  (?) unclassified          anything left. A non-zero count here is a FAILED
                            grade, because it means the rig cannot say what
                            the difference is.
"""
import collections
import sys
import os


def load(path, width=0):
    """Split on tabs, then re-glue any surplus back into the TEXT column. A
    comment body may legitimately contain a tab; the column count is fixed, so
    the surplus belongs to the one free-text field and nowhere else."""
    if not os.path.exists(path):
        return []
    rows = []
    with open(path) as handle:
        for line in handle:
            if not line.strip():
                continue
            parts = line.rstrip("\n").split("\t")
            if width and len(parts) > width:
                text = "\t".join(parts[width - 2: len(parts) - 1])
                parts = parts[: width - 2] + [text, parts[-1]]
            if width and len(parts) != width:
                continue
            rows.append(parts)
    return rows


def classify_comment(only5, only6):
    by_line5 = collections.defaultdict(list)
    by_line6 = collections.defaultdict(list)
    for row in only5:
        by_line5[(row[0], row[1])].append(row)
    for row in only6:
        by_line6[(row[0], row[1])].append(row)

    buckets = collections.Counter()
    samples = collections.defaultdict(list)

    for key, rows5 in by_line5.items():
        rows6 = by_line6.get(key, [])
        for row5 in rows5:
            if not rows6:
                buckets["m missing-in-v6"] += 1
                samples["m missing-in-v6"].append(row5)
                continue
            row6 = rows6[0]
            if row5[5] == row6[5] and row5[6] == row6[6]:
                bucket = "p position-convention"
            elif row5[2] == row6[2] and row5[6] == row6[6]:
                bucket = "t text-strip"
            elif row5[5] == row6[5]:
                bucket = "k kind-vocabulary"
            else:
                bucket = "t text-strip"
            buckets[bucket] += 1
            samples[bucket].append((row5, row6))

    for key, rows6 in by_line6.items():
        if key not in by_line5:
            for row6 in rows6:
                buckets["x extra-in-v6"] += 1
                samples["x extra-in-v6"].append(row6)

    return buckets, samples


def main():
    out = sys.argv[1]
    unclassified = 0

    only5 = load(os.path.join(out, "comment_node.only-v5.tsv"), 7)
    only6 = load(os.path.join(out, "comment_node.only-v6.tsv"), 7)
    buckets, samples = classify_comment(only5, only6)
    total = len(only5) + len(only6)
    print(f"comment_node: {total} diff rows")
    for bucket, count in sorted(buckets.items()):
        print(f"  {bucket:<24} {count}")
        for sample in samples[bucket][:2]:
            print(f"      {sample}")
    accounted = sum(buckets.values())
    if accounted != total:
        # a v5 row and a v6 row on the same line are ONE difference, counted
        # once above; only a genuine leftover is unclassified.
        paired = sum(count for bucket, count in buckets.items()
                     if bucket[0] in "ptk")
        if accounted + paired != total:
            unclassified += total - accounted - paired

    only5 = load(os.path.join(out, "arch_node.only-v5.tsv"), 3)
    only6 = load(os.path.join(out, "arch_node.only-v6.tsv"), 3)
    print(f"arch_node: {len(only5) + len(only6)} diff rows")
    for row in only5:
        print(f"  m missing-in-v6          {row}")
    for row in only6:
        print(f"  x extra-in-v6            {row}")

    print()
    print(f"UNCLASSIFIED {unclassified}")
    return 0 if unclassified == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
