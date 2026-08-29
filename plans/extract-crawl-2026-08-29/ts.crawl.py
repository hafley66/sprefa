#!/usr/bin/env python3
"""BFS the resolved call graph of a TypeScript corpus from its package entrypoints.

Inputs, all produced by v6/sprefa-extract/target/release/extract:
  --edges  JSONL from `extract --resolve --family call <every file>`
  --defs   TSV `path\t<json>` from `extract --family call <file>` per file
  --types  TSV `path\t<json>` from `extract --family type <file>` per file
  --entries  newline list of entry module paths (package.json exports map)

Edges key on (path, name); a file with two defs of one name collapses to one
graph node, and the collapse count is reported.
"""
import argparse
import collections
import json
import os
import re
import sys

DEF_KINDS = {"function", "method"}


def load_defs(path):
    """(path, name) -> list of spans, from call-plane node rows."""
    defs = collections.defaultdict(list)
    for line in open(path):
        p, j = line.split("\t", 1)
        d = json.loads(j)
        if d.get("record") != "node" or d.get("kind") not in DEF_KINDS:
            continue
        if not d.get("name"):
            continue
        defs[(p, d["name"])].append((d["span"]["start"], d["span"]["end"], d["kind"]))
    return defs


def load_sites(path):
    """path -> call-site count, and path -> unresolved row count."""
    sites = collections.Counter()
    unresolved = collections.Counter()
    for line in open(path):
        p, j = line.split("\t", 1)
        d = json.loads(j)
        if d.get("record") == "site":
            sites[p] += 1
        elif d.get("record") == "unresolved":
            unresolved[p] += 1
    return sites, unresolved


def load_classes(path):
    """path -> list of (start, end, name) for type-plane class nodes."""
    classes = collections.defaultdict(list)
    for line in open(path):
        p, j = line.split("\t", 1)
        d = json.loads(j)
        if d.get("record") == "node" and d.get("kind") == "class" and d.get("name"):
            classes[p].append((d["span"]["start"], d["span"]["end"], d["name"]))
    return classes


def load_named_spans(path):
    """path -> spans of every NAMED call-plane node, smallest first."""
    spans = collections.defaultdict(list)
    for line in open(path):
        p, j = line.split("\t", 1)
        d = json.loads(j)
        if d.get("record") == "node" and d.get("name"):
            spans[p].append((d["span"]["start"], d["span"]["end"], d["name"]))
    for p in spans:
        spans[p].sort(key=lambda t: t[1] - t[0])
    return spans


CLOSURE = re.compile(r"^closure@(\d+)$")


def load_edges(path, named_spans=None):
    """caller (path, name) -> set of callee (path, name); plus counts.

    `resolved_edge.caller_name` is `closure@<def span start>` for a lambda body,
    and the call plane names no lambda, so the caller has no `node` row to join.
    With `named_spans` the offset is re-homed onto the innermost NAMED def that
    covers it; a lambda no named def covers is counted and dropped.
    """
    out = collections.defaultdict(set)
    total = closures = orphan = 0
    for line in open(path):
        d = json.loads(line)
        if d.get("record") != "resolved_edge":
            continue
        total += 1
        cp, cn = d["caller_path"], d["caller_name"]
        m = CLOSURE.match(cn)
        if m:
            closures += 1
            if named_spans is not None:
                off = int(m.group(1))
                hit = next(
                    (n for a, b, n in named_spans.get(cp, ()) if a <= off < b), None
                )
                if hit is None:
                    orphan += 1
                    continue
                cn = hit
        out[(cp, cn)].add((d["callee_path"], d["callee_name"]))
    return out, total, closures, orphan


STAR_RE = re.compile(r'^export \* from ["\'](.+?)["\'];', re.M)
NAMED_RE = re.compile(
    r"^export (?:declare )?(?:async )?(?:function|class|const|let|var) (\w+)", re.M
)


def expand_entries(corpus, entries):
    """Follow one level of `export * from` so a barrel names real modules."""
    seen = list(entries)
    out = set(entries)
    while seen:
        e = seen.pop()
        p = os.path.join(corpus, e)
        if not os.path.exists(p):
            continue
        src = open(p, encoding="utf8", errors="replace").read()
        for spec in STAR_RE.findall(src):
            if not spec.startswith("."):
                continue
            tgt = os.path.normpath(os.path.join(os.path.dirname(e), spec))
            if tgt not in out and os.path.exists(os.path.join(corpus, tgt)):
                out.add(tgt)
                seen.append(tgt)
    return sorted(out)


def roots_for(corpus, entry_files, defs, classes):
    """Exported callables in the entry modules, plus methods of exported classes."""
    roots = set()
    for e in entry_files:
        p = os.path.join(corpus, e)
        if not os.path.exists(p):
            continue
        src = open(p, encoding="utf8", errors="replace").read()
        names = set(NAMED_RE.findall(src))
        for n in names:
            if (e, n) in defs:
                roots.add((e, n))
        exported_class_spans = [
            (s, en) for (s, en, cn) in classes.get(e, []) if cn in names
        ]
        for (path, name), spans in defs.items():
            if path != e:
                continue
            for st, en, kind in spans:
                if kind != "method":
                    continue
                if any(cs <= st and en <= ce for cs, ce in exported_class_spans):
                    roots.add((path, name))
                    break
    return roots


def bfs(roots, edges, defs):
    """Depth of every reachable def; roots sit at depth 0."""
    depth = {r: 0 for r in roots if r in defs}
    queue = collections.deque(depth)
    while queue:
        cur = queue.popleft()
        for nxt in edges.get(cur, ()):
            if nxt in depth or nxt not in defs:
                continue
            depth[nxt] = depth[cur] + 1
            queue.append(nxt)
    return depth


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--edges", required=True)
    ap.add_argument("--defs", required=True)
    ap.add_argument("--types", required=True)
    ap.add_argument("--entries", required=True)
    ap.add_argument("--label", default="crawl")
    ap.add_argument("--json-out")
    ap.add_argument(
        "--fold-closures",
        action="store_true",
        help="re-home a closure@N caller onto the innermost named def covering N",
    )
    args = ap.parse_args()

    defs = load_defs(args.defs)
    classes = load_classes(args.types)
    spans = load_named_spans(args.defs) if args.fold_closures else None
    edges, edge_rows, closure_callers, orphan_closures = load_edges(args.edges, spans)
    sites, unresolved = load_sites(args.defs)

    entries = [l.strip() for l in open(args.entries) if l.strip()]
    entry_files = expand_entries(args.corpus, entries)
    roots = roots_for(args.corpus, entry_files, defs, classes)
    depth = bfs(roots, edges, defs)

    span_of = lambda k: max(e - s for s, e, _ in defs[k])
    hist = collections.Counter(depth.values())
    out_deg = {k: len(v) for k, v in edges.items() if k in defs}
    unreached = [k for k in defs if k not in depth]

    report = {
        "label": args.label,
        "entry_modules": entry_files,
        "roots": len(roots),
        "fold_closures": args.fold_closures,
        "edge_rows": edge_rows,
        "edges_with_closure_caller": closure_callers,
        "closure_callers_no_named_parent": orphan_closures,
        "graph_nodes_with_out_edges": len(edges),
        "defs_total": len(defs),
        "defs_with_duplicate_name": sum(1 for v in defs.values() if len(v) > 1),
        "def_spans_total": sum(len(v) for v in defs.values()),
        "reachable": len(depth),
        "unreachable": len(unreached),
        "depth_histogram": dict(sorted(hist.items())),
        "max_depth": max(hist) if hist else 0,
        "top_out_degree": [
            {"path": k[0], "name": k[1], "out": v}
            for k, v in sorted(out_deg.items(), key=lambda kv: -kv[1])[:20]
        ],
        "largest_unreachable": [
            {"path": k[0], "name": k[1], "span": span_of(k)}
            for k in sorted(unreached, key=span_of, reverse=True)[:20]
        ],
        "sites_total": sum(sites.values()),
        "unresolved_rows": sum(unresolved.values()),
    }
    text = json.dumps(report, indent=1)
    if args.json_out:
        open(args.json_out, "w").write(text + "\n")
    print(text)


if __name__ == "__main__":
    sys.exit(main())
