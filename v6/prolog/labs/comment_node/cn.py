#!/usr/bin/env python3
"""cn.py -- ROUTE (b)'s whole cost, in one policy-free tool.

Two modes, neither of which knows a single marker convention. That split is
deliberate and it is the law `std/suppress.dl`'s own header states:

    "Policy lives HERE, never in Rust -- the engine only produces
     grammar-accurate comment facts; which directives mean what, and how a
     block pairs, is all datalog."

So this tool ships exactly what the grammar knows and nothing else. `ARCH`,
`dl-disable-line`, `TODO`, `README`, `BEGIN:` never appear below.

  cn.py comments PATH
      the v5 `comment_node` row, from the cst family this repo already emits:
      {path, line, col, end_line, end_col, kind, comment_text}; line 1-based
      and col 0-based, matching `src/cst.rs` exactly; tokens stripped;
      string-literal-safe because the SPANS come from the grammar and not
      from a scan.

  cn.py lines PATH
      the generic byte-span FLATTENER: reads the extractor's own JSONL on
      stdin and re-emits each record with `line`/`col`/`end_line`/`end_col`
      lifted to TOP-LEVEL int fields beside the record's other top-level keys.
      This is the answer to the nested-span wall three separate arcs have hit
      (flagship-callgraph.dl6 dropped `line`; diag-rail.dl6 shipped whole-file
      zeros; ARCH row struct_host_output_seam is the compiler half): the
      extractor writes `"span":{"start":..,"end":..}` and
      `serve/1_hosts.ts decodeObjectItems` is a projection over TOP-LEVEL
      declared columns, so a nested field can never reach a declared int
      column. One flattener in the host template unblocks line numbers for
      EVERY family at once and touches neither the extractor nor the
      struct-typed-host-output seam.

Both modes take the file path so the byte->line index is built from the real
bytes, never guessed.
"""
import json
import sys


# Grammar comment-token prefixes, longest first so `///` wins over `//`. This
# is LEXICAL, a property of each grammar's comment syntax, not policy -- the
# same place v5 puts it (the extractor strips before the fact ever exists).
STRIP_PREFIXES = ("///", "//!", "/**", "//", "/*", "#!", "%%", "#", "%", "--")
STRIP_SUFFIXES = ("*/",)


def strip_tokens(text):
    body = text.strip()
    for prefix in STRIP_PREFIXES:
        if body.startswith(prefix):
            body = body[len(prefix):]
            break
    for suffix in STRIP_SUFFIXES:
        if body.endswith(suffix):
            body = body[: -len(suffix)]
            break
    # a block comment's continuation asterisks are the same lexical noise
    return body.strip().lstrip("*").strip()


class LineIndex:
    """Byte offset -> (line, col). SLOT-SPAN-UNITS, settled against the v5
    contract rather than invented: `src/cst.rs walk_comments` normalizes
    tree-sitter's 0-based row/col to 1-BASED LINE and 0-BASED COLUMN, and
    `end_row`/`end_col` come from the node's END position, which is the
    position AFTER the last byte. Emitting anything else here would make the
    parity diff measure the convention instead of the technique."""

    def __init__(self, data):
        self.starts = [0]
        for index, byte in enumerate(data):
            if byte == 0x0A:
                self.starts.append(index + 1)

    def locate(self, offset):
        low, high = 0, len(self.starts) - 1
        while low < high:
            mid = (low + high + 1) // 2
            if self.starts[mid] <= offset:
                low = mid
            else:
                high = mid - 1
        return low + 1, offset - self.starts[low]


def comment_kind(kind):
    """SLOT-COMMENT-KIND-VOCAB, decided by measurement over the real streams:
    tree-sitter's names are grammar-local (`line_comment`, `block_comment`,
    `comment`, `doc_comment`) and the v5 line|block|doc vocabulary is a
    two-line fold over them. `doc` is the one that is NOT a kind in the
    grammar: rust spells `/// x` as a line_comment node carrying a
    `doc_comment` CHILD, so doc-ness is a parent/child relation. Reading it
    off the token instead keeps the fold local and language-independent."""
    if kind.startswith("block") or kind == "comment_block":
        return "block"
    return "line"


def doc_kind(raw_text):
    stripped = raw_text.strip()
    if stripped.startswith("///") or stripped.startswith("//!"):
        return "doc"
    if stripped.startswith("/**") and not stripped.startswith("/***"):
        return "doc"
    return None


def mode_comments(path):
    with open(path, "rb") as handle:
        data = handle.read()
    index = LineIndex(data)

    spans = []
    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        row = json.loads(raw)
        if row.get("record") != "node":
            continue
        kind = row.get("kind") or ""
        if not kind.endswith("comment"):
            continue
        spans.append((row["span"]["start"], row["span"]["end"], kind))

    # A COMMENT IS A LEAF. `src/cst.rs walk_comments` stops descending the
    # moment a node's kind contains "comment" (`continue` in its DFS), so v5
    # emits ONE row for `/// x` even though the rust grammar nests a
    # `doc_comment` child inside the `line_comment`. The cst family is
    # lossless and reports both, so the same rule has to be applied here or
    # every rust doc line doubles. Measured before it was fixed: 430 v6 rows
    # against 254 v5 rows on the pinned corpus, the whole gap being nested
    # `doc_comment` children.
    outer = []
    for start, end, kind in spans:
        if any(other_start <= start and end <= other_end and (other_start, other_end) != (start, end)
               for other_start, other_end, _ in spans):
            continue
        outer.append((start, end, kind))

    for start, end, kind in sorted(outer):
        text = data[start:end].decode("utf-8", "replace")
        line, col = index.locate(start)
        end_line, end_col = index.locate(end)
        print(json.dumps({
            "path": path,
            "line": line, "col": col,
            "end_line": end_line, "end_col": end_col,
            "kind": doc_kind(text) or comment_kind(kind),
            "comment_text": strip_tokens(text),
        }, separators=(",", ":")))


def mode_lines(path):
    with open(path, "rb") as handle:
        data = handle.read()
    index = LineIndex(data)
    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        row = json.loads(raw)
        span = row.pop("span", None)
        if not isinstance(span, dict):
            continue
        line, col = index.locate(span["start"])
        end_line, end_col = index.locate(span["end"])
        row["path"] = path
        row["line"], row["col"] = line, col
        row["end_line"], row["end_col"] = end_line, end_col
        row["start"], row["end"] = span["start"], span["end"]
        print(json.dumps({key: value for key, value in row.items() if value is not None},
                         separators=(",", ":")))


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("comments", "lines"):
        print("usage: cn.py comments|lines PATH  (extractor JSONL on stdin)", file=sys.stderr)
        return 2
    (mode_comments if sys.argv[1] == "comments" else mode_lines)(sys.argv[2])
    return 0


if __name__ == "__main__":
    sys.exit(main())
