#!/usr/bin/env python3
"""Policy-free projections for the cst family and byte-span records."""

import json
import re
import sys


STRIP_PREFIXES = ("///", "//!", "/**", "//", "/*", "#!", "%%", "#", "%", "--")

PROSE_RE = re.compile(r"[A-Za-z]")


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


def outer_comment_spans():
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
    return sorted(outer)


def physical_line_text(data, index, line_number):
    starts = index.starts
    if line_number < 1 or line_number > len(starts):
        return ""
    start = starts[line_number - 1]
    end = starts[line_number] if line_number < len(starts) else len(data)
    return data[start:end].decode("utf-8", "replace").rstrip("\r\n")


def prose_flag(text, line_number):
    body = text.strip()
    if line_number == 1 and body.startswith("#!"):
        return False
    return bool(PROSE_RE.search(strip_tokens(text)))


def comments(path):
    data, index = read_input(path)
    for start, end, kind in outer_comment_spans():
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


def comment_lines(path):
    data, index = read_input(path)
    prose = {}
    for start, end, _kind in outer_comment_spans():
        last_byte = max(end - 1, start)
        line, _ = index.locate(start)
        end_line, _ = index.locate(last_byte)
        for line_number in range(line, end_line + 1):
            text = physical_line_text(data, index, line_number)
            prose[line_number] = prose_flag(text, line_number)
    seq = 0
    for line_number in sorted(prose):
        if prose[line_number]:
            seq += 1
        print(json.dumps({
            "path": path,
            "line": line_number,
            "prose_flag": 1 if prose[line_number] else 0,
            "prose_seq": seq,
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
    if len(sys.argv) != 3 or sys.argv[1] not in ("comments", "comment-lines", "lines"):
        print("usage: comment_node.py comments|comment-lines|lines PATH", file=sys.stderr)
        return 2
    verb = sys.argv[1]
    if verb == "comments":
        comments(sys.argv[2])
    elif verb == "comment-lines":
        comment_lines(sys.argv[2])
    else:
        lines(sys.argv[2])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
