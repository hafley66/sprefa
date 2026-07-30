#!/usr/bin/env python3
"""Policy-free projections for the cst family and byte-span records."""

import json
import sys


STRIP_PREFIXES = ("///", "//!", "/**", "//", "/*", "#!", "%%", "#", "%", "--")


def strip_tokens(text):
    body = text.strip()
    for prefix in STRIP_PREFIXES:
        if body.startswith(prefix):
            body = body[len(prefix):]
            break
    if body.endswith("*/"):
        body = body[:-2]
    return body.strip().lstrip("*").strip()


class LineIndex:
    def __init__(self, data):
        self.starts = [0]
        for index, byte in enumerate(data):
            if byte == 0x0A:
                self.starts.append(index + 1)

    def locate(self, offset):
        low, high = 0, len(self.starts) - 1
        while low < high:
            middle = (low + high + 1) // 2
            if self.starts[middle] <= offset:
                low = middle
            else:
                high = middle - 1
        return low + 1, offset - self.starts[low]


def comment_kind(kind):
    return "block" if kind.startswith("block") or kind == "comment_block" else "line"


def doc_kind(text):
    body = text.strip()
    if body.startswith(("///", "//!")):
        return "doc"
    if body.startswith("/**") and not body.startswith("/***"):
        return "doc"
    return None


def read_input(path):
    with open(path, "rb") as handle:
        data = handle.read()
    return data, LineIndex(data)


def comments(path):
    data, index = read_input(path)
    spans = []
    for raw in sys.stdin:
        if not raw.strip():
            continue
        row = json.loads(raw)
        if row.get("record") != "node":
            continue
        kind = row.get("kind") or ""
        if kind.endswith("comment"):
            span = row["span"]
            spans.append((span["start"], span["end"], kind))

    outer = []
    for start, end, kind in spans:
        nested = any(
            other_start <= start
            and end <= other_end
            and (other_start, other_end) != (start, end)
            for other_start, other_end, _ in spans
        )
        if not nested:
            outer.append((start, end, kind))

    for start, end, kind in sorted(outer):
        text = data[start:end].decode("utf-8", "replace")
        line, col = index.locate(start)
        end_line, end_col = index.locate(end)
        print(json.dumps({
            "path": path,
            "line": line,
            "col": col,
            "end_line": end_line,
            "end_col": end_col,
            "kind": doc_kind(text) or comment_kind(kind),
            "comment_text": strip_tokens(text),
        }, separators=(",", ":")))


def lines(path):
    data, index = read_input(path)
    for raw in sys.stdin:
        if not raw.strip():
            continue
        row = json.loads(raw)
        span = row.pop("span", None)
        if not isinstance(span, dict):
            continue
        line, col = index.locate(span["start"])
        end_line, end_col = index.locate(span["end"])
        row.update({
            "path": path,
            "line": line,
            "col": col,
            "end_line": end_line,
            "end_col": end_col,
            "start": span["start"],
            "end": span["end"],
        })
        print(json.dumps({key: value for key, value in row.items() if value is not None}, separators=(",", ":")))


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("comments", "lines"):
        print("usage: comment_node.py comments|lines PATH", file=sys.stderr)
        return 2
    (comments if sys.argv[1] == "comments" else lines)(sys.argv[2])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
