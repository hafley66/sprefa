#!/usr/bin/env python3
"""Convert jelly (@cs-au-dk/jelly 0.13.0) call-graph JSON chunks to the bench
lab 4-column call tsv: `src_path  src_name  dst_path  dst_name`, paths
relative to the corpus root, names bare (`<module>` for top-level callers,
`Class.method` for methods), matching ts5.oracle.call.tsv / ts.codeql2.call.tsv
conventions (plans/extract-bench-2026-08-29/COMMON.md).

Jelly's JSON gives functions as `fileIdx:startLine:startCol:endLine:endCol`
(1-based) with no names, so names are recovered from the source text at the
declaration site. Chunks are unioned and deduplicated; cross-chunk edges that
jelly never saw are a known caveat (each chunk was a separate analysis).

Usage:
  jelly_convert.py --corpus-root /path/to/TypeScript-5.9 \
      --chunk name=/tmp/jelly_name.json [--chunk ...] --out ts5.jelly.call.tsv
"""

import argparse
import json
import re
from pathlib import Path

IDENT = r"[A-Za-z_$][\w$]*"
RE_FUNC_DECL = re.compile(rf"function\s+({IDENT})")
RE_CONSTRUCTOR = re.compile(r"\bconstructor\b")
RE_CLASS = re.compile(rf"\bclass\s+({IDENT})")
RE_GET_SET = re.compile(rf"\b(?:get|set)\s+({IDENT})")
RE_ASSIGN = re.compile(rf"({IDENT})\s*=\s*(?:async\s+)?(?:function|\()")
RE_METHOD = re.compile(rf"({IDENT})\s*\(")
RE_PROP = re.compile(rf"({IDENT})\s*:")


def wants_ts5(rel: str) -> bool:
    """tests/bench/mod.rs `wants("ts5", rel)`: src/** minus src/lib, .ts only."""
    parts = rel.split("/")
    return (
        len(parts) >= 1
        and parts[0] == "src"
        and not (len(parts) >= 2 and parts[1] == "lib")
        and rel.endswith(".ts")
    )


def function_name(lines, loc):
    """Recover a bare function name from the declaration source text."""
    sl, sc = loc["sl"], loc["sc"]
    if sl < 1 or sl > len(lines):
        return None
    line = lines[sl - 1]
    tail = line[sc - 1 :] if sc - 1 <= len(line) else ""
    m = RE_FUNC_DECL.search(tail)
    if m:
        return m.group(1)
    if RE_CONSTRUCTOR.search(tail[:40]):
        for up in range(sl - 1, 0, -1):
            cm = RE_CLASS.search(lines[up - 1])
            if cm:
                return f"{cm.group(1)}.constructor"
        return "constructor"
    m = RE_GET_SET.search(tail)
    if m:
        return m.group(1)
    head = line[: sc - 1]
    for regex in (RE_ASSIGN, RE_METHOD, RE_PROP):
        matches = list(regex.finditer(head))
        if matches:
            return matches[-1].group(1)
    m = RE_METHOD.search(tail)
    if m:
        return m.group(1)
    return None


def parse_functions(raw_functions, files, src_cache, corpus_root):
    """id -> (rel_path, name) or (rel_path, None); module pseudo-functions
    (a span starting at 1:1 covering the file) map to the name `<module>`."""
    funcs = {}
    for fid, spec in raw_functions.items():
        parts = spec.split(":")
        fidx, sl, sc, el, ec = (int(p) for p in parts)
        rel = files[int(fidx)]
        if sl == 1 and sc == 1:
            funcs[int(fid)] = (rel, "<module>")
            continue
        path = corpus_root / rel
        if path not in src_cache:
            try:
                src_cache[path] = path.read_text(encoding="utf-8", errors="replace").splitlines()
            except OSError:
                src_cache[path] = []
        name = function_name(src_cache[path], {"sl": sl, "sc": sc})
        funcs[int(fid)] = (rel, name)
    return funcs


def convert_chunk(json_path, corpus_root, src_cache):
    data = json.loads(Path(json_path).read_text(encoding="utf-8"))
    files = data["files"]
    funcs = parse_functions(data["functions"], files, src_cache, corpus_root)
    rows = set()
    dropped = 0
    for caller, callee in data["fun2fun"]:
        c_rel, c_name = funcs.get(int(caller), (None, None))
        d_rel, d_name = funcs.get(int(callee), (None, None))
        if c_rel is None or d_rel is None:
            dropped += 1
            continue
        if c_name is None or d_name is None:
            dropped += 1
            continue
        if not (wants_ts5(c_rel) and wants_ts5(d_rel)):
            continue
        rows.add(f"{c_rel}\t{c_name}\t{d_rel}\t{d_name}")
    return rows, dropped, len(files)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--corpus-root", required=True)
    parser.add_argument("--chunk", action="append", required=True, metavar="NAME=JSON")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    corpus_root = Path(args.corpus_root).resolve()
    src_cache = {}
    all_rows = set()
    total_dropped = 0
    total_files = 0
    for chunk in args.chunk:
        name, _, json_path = chunk.partition("=")
        rows, dropped, nfiles = convert_chunk(json_path, corpus_root, src_cache)
        all_rows |= rows
        total_dropped += dropped
        total_files += nfiles
        print(f"chunk {name}: files={nfiles} rows={len(rows)} dropped_edges={dropped}")
    out = Path(args.out)
    with out.open("w", encoding="utf-8") as handle:
        for row in sorted(all_rows):
            handle.write(row + "\n")
    print(f"union: files_seen={total_files} rows={len(all_rows)} dropped_edges={total_dropped} -> {out}")


if __name__ == "__main__":
    main()
