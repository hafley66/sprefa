#!/usr/bin/env python3
"""slice_comments.py -- route (a)'s output shape, produced OUTSIDE the extractor.

Reads the cst JSONL the extractor already emits, keeps the comment nodes, and
does the byte slice + line/col computation + token strip that a real `comment`
family would do inside rust. Emitting the identical shape from here is what
lets route (a) be PRICED without the extractor being touched: the row count,
the byte size, and the exact column set are all real, and the only thing left
unpriced is the rust diff itself.

Kind vocabulary mapping (SLOT-COMMENT-KIND-VOCAB, measured not guessed):
tree-sitter emits grammar-local names. The three v5 kinds fall out of two
facts, both visible in the cst stream:
  * a node whose kind ENDS in `comment` is a comment;
  * `doc` is a PARENT/CHILD relation, not a kind: rust's `/// x` is a
    line_comment node carrying a `doc_comment` child, so doc-ness is an
    ordinary join on the cst child edges, not a new column.
This script computes kind by the same rule so the price includes it.
"""
import json
import sys

STRIP_PREFIXES = ("///", "//!", "//", "/**", "/*", "#!", "#", "%%", "%")
STRIP_SUFFIXES = ("*/",)


def strip_tokens(text: str) -> str:
    body = text.strip()
    for prefix in STRIP_PREFIXES:
        if body.startswith(prefix):
            body = body[len(prefix):]
            break
    for suffix in STRIP_SUFFIXES:
        if body.endswith(suffix):
            body = body[: -len(suffix)]
            break
    return body.strip()


def line_col_index(data: bytes):
    """Byte offset -> (line, col), both 1-based, v5's own convention."""
    starts = [0]
    for index, byte in enumerate(data):
        if byte == 0x0A:
            starts.append(index + 1)
    return starts


def locate(starts, offset: int):
    low, high = 0, len(starts) - 1
    while low < high:
        mid = (low + high + 1) // 2
        if starts[mid] <= offset:
            low = mid
        else:
            high = mid - 1
    return low + 1, offset - starts[low] + 1


def main() -> int:
    files_path, cst_path = sys.argv[1], sys.argv[2]
    with open(files_path) as handle:
        paths = [line.strip() for line in handle if line.strip()]

    # cst.jsonl is the concatenation of per-file runs in the same order, and a
    # `source_file` node opens each one, so the file boundary is recoverable.
    contents = {}
    for path in paths:
        with open(path, "rb") as handle:
            contents[path] = handle.read()

    current = None
    index = None
    cursor = -1
    with open(cst_path) as handle:
        for raw in handle:
            row = json.loads(raw)
            if row.get("record") != "node":
                continue
            kind = row.get("kind") or ""
            if kind == "source_file":
                cursor += 1
                current = paths[cursor]
                index = line_col_index(contents[current])
                continue
            if not kind.endswith("comment"):
                continue
            if kind.endswith("comment_marker"):
                continue
            span = row["span"]
            data = contents[current][span["start"]: span["end"]]
            text = data.decode("utf-8", "replace")
            line, col = locate(index, span["start"])
            end_line, end_col = locate(index, span["end"])
            print(json.dumps({
                "record": "comment",
                "family": "comment",
                "path": current,
                "span": {"start": span["start"], "end": span["end"]},
                "line": line, "col": col,
                "end_line": end_line, "end_col": end_col,
                "kind": kind,
                "text": strip_tokens(text),
            }, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
