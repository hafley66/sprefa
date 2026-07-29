#!/usr/bin/env python3
"""comment_lines.py -- read one file's cst JSONL on stdin, print the 1-based
line numbers that carry a grammar-backed comment node, one per line.

This is the GRAMMAR TRUTH side of string-safety.sh: it exists so the naive
scanner's flagged lines can be differenced against what the parser actually
calls a comment.
"""
import json
import sys


def main() -> int:
    with open(sys.argv[1], "rb") as handle:
        data = handle.read()
    starts = [0]
    for index, byte in enumerate(data):
        if byte == 0x0A:
            starts.append(index + 1)

    def line_of(offset: int) -> int:
        low, high = 0, len(starts) - 1
        while low < high:
            mid = (low + high + 1) // 2
            if starts[mid] <= offset:
                low = mid
            else:
                high = mid - 1
        return low + 1

    lines = set()
    for raw in sys.stdin:
        row = json.loads(raw)
        if row.get("record") != "node":
            continue
        kind = row.get("kind") or ""
        if not kind.endswith("comment"):
            continue
        span = row["span"]
        for line in range(line_of(span["start"]), line_of(max(span["end"] - 1, span["start"])) + 1):
            lines.add(line)
    for line in sorted(lines):
        print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main())
