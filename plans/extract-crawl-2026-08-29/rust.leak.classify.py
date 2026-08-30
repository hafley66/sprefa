#!/usr/bin/env python3
"""Classify the rust call LEAK: oracle rows (projected) that our resolve pass
does not emit.

Input:
  --leak     tsv of leak rows (src_path src_name dst_path dst_name)
  --percall  jsonl from a per-file `extract --family call <f>` sweep, each line
             prefixed with `"__f":"<rel path>"` (node / site / method_owner)
  --resolve  jsonl from the one-process `extract --resolve --family call,type`
             run (resolved_edge / unresolved)
  --root     corpus root, for reading source text

Every leak row is joined to the call SITES our parse found inside the named
caller whose callee name matches. The row's tier is then:

  no_site     we saw no such call site inside that caller at all
  misbind     the site bound, to another file
  drop:<r>    the site dropped, with our own reason slug

Sites in the `drop` and `no_site` tiers get a shape class read off the source
text and the file's own def tables. Prints a tier table, a class table, and
(with --sample N --seed S) writes a sample tsv with one row per leak row.
"""
import argparse
import json
import os
import random
import re
from collections import Counter, defaultdict


def load_percall(path):
    nodes = defaultdict(list)
    sites = defaultdict(list)
    owners = defaultdict(list)
    specifiers = defaultdict(list)
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            rel = row["__f"]
            record = row["record"]
            if record == "node":
                nodes[rel].append(row)
            elif record == "site":
                sites[rel].append(row)
            elif record == "method_owner":
                owners[rel].append(row)
            elif record == "specifier":
                specifiers[rel].append(row)
    return nodes, sites, owners, specifiers


def load_resolve(path):
    bound = defaultdict(list)
    drops = {}
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            if row["record"] == "resolved_edge":
                key = (row["caller_path"], row.get("caller_site_start"))
                bound[key].append(row)
            elif row["record"] == "unresolved" and row["family"] == "call":
                drops[(row["path"], row["span"]["start"])] = row
    return bound, drops


def covering(nodes_in_file, start, end):
    """The innermost named def covering [start, end), the resolve pass's own
    caller-binding rule."""
    best = None
    for node in nodes_in_file:
        span = node["span"]
        if span["start"] <= start and end <= span["end"] and node.get("name"):
            if best is None or span["end"] - span["start"] < best["span"]["end"] - best["span"]["start"]:
                best = node
    return best


TRAIT_RETURN = re.compile(r"->\s*impl\s+")
GENERIC_CALL = re.compile(r"::<")


def receiver_text(text, site_start):
    """The expression text immediately left of the call site, balanced over
    `)`/`]`, capped at 120 bytes: the receiver peel section 16 used."""
    i = site_start
    if i <= 0 or text[i - 1 : i] != ".":
        return ""
    i -= 1
    depth = 0
    start = i
    while start > 0 and site_start - start < 120:
        ch = text[start - 1]
        if ch in ")]":
            depth += 1
        elif ch in "([":
            if depth == 0:
                break
            depth -= 1
        elif depth == 0 and not (ch.isalnum() or ch in "_.:?&*<>"):
            break
        start -= 1
    return text[start:i]


def enclosing_signature(text, node_span):
    head = text[node_span["start"] : min(node_span["end"], node_span["start"] + 400)]
    brace = head.find("{")
    return head[:brace] if brace >= 0 else head


def classify(row, text, node, site, drop, owners_in_file, specifiers_in_file, corpus_types):
    """One shape class per leak row. Order matters: the first rule that fires
    wins, most specific first."""
    src_path, caller, dst_path, callee = row
    if site is None:
        # No parse site: a macro body we do not expand, or a caller name that
        # is not one of our defs.
        if node is None:
            return "caller_def_absent"
        return "site_absent_macro_or_shape"
    start = site["span"]["start"]
    call_path = site.get("callee_path") or ""
    recv = receiver_text(text, start)
    sig = enclosing_signature(text, node["span"]) if node else ""
    reason = drop["reason"] if drop else "bound_elsewhere"

    if call_path.startswith("Self::"):
        return "assoc_through_Self"
    if GENERIC_CALL.search(text[max(0, start - 60) : start + len(callee) + 40]):
        return "generic_instantiation"
    if recv.startswith("self."):
        return "method_on_own_field"
    if recv in ("self", ""):
        if not recv and call_path:
            head = call_path.split("::")[0]
            if head and head[0].isupper():
                return "assoc_fn_on_type"
            return "module_qualified_path"
        if not recv:
            return "free_fn_bare_name"
        return "method_on_self"
    if recv.endswith(")"):
        return "receiver_is_call_result"
    if recv.endswith("]"):
        return "receiver_is_index"
    if TRAIT_RETURN.search(sig):
        return "caller_returns_impl_trait"
    if "|" in recv:
        return "closure_param"
    if recv.split(".")[0] in corpus_types:
        return "receiver_is_corpus_type_path"
    return f"receiver_local_{reason}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--leak", required=True)
    ap.add_argument("--percall", required=True)
    ap.add_argument("--resolve", required=True)
    ap.add_argument("--root", required=True)
    ap.add_argument("--sample", type=int, default=0)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--out")
    args = ap.parse_args()

    nodes, sites, owners, specifiers = load_percall(args.percall)
    bound, drops = load_resolve(args.resolve)

    corpus_types = set()
    for rows in owners.values():
        for row in rows:
            corpus_types.add(row["self_type"])

    leak = [line.split("\t") for line in open(args.leak).read().splitlines() if line]
    leak = [row for row in leak if len(row) == 4]
    if args.sample:
        random.Random(args.seed).shuffle(leak)
        leak = leak[: args.sample]

    text_cache = {}

    def source(rel):
        if rel not in text_cache:
            try:
                with open(os.path.join(args.root, rel), encoding="utf-8", errors="replace") as fh:
                    text_cache[rel] = fh.read()
            except OSError:
                text_cache[rel] = ""
        return text_cache[rel]

    tiers = Counter()
    classes = Counter()
    class_examples = defaultdict(list)
    out_rows = []
    for row in leak:
        src_path, caller, dst_path, callee = row
        text = source(src_path)
        candidates = [
            site
            for site in sites.get(src_path, [])
            if site["callee"] == callee
            and (covering(nodes.get(src_path, []), site["span"]["start"], site["span"]["end"]) or {}).get("name")
            == caller
        ]
        # A caller may call the same name at several sites; the leak row is one
        # row for all of them, so a dropped site outranks a bound one.
        dropped = [s for s in candidates if (src_path, s["span"]["start"]) in drops]
        site = (dropped or candidates or [None])[0]
        node = (
            covering(nodes.get(src_path, []), site["span"]["start"], site["span"]["end"])
            if site
            else None
        )
        drop = drops.get((src_path, site["span"]["start"])) if site else None
        if site is None:
            tier = "no_site"
        elif drop is not None:
            tier = "drop:" + drop["reason"]
        else:
            edges = bound.get((src_path, site["span"]["start"]), [])
            tier = "misbind" if edges else "no_edge_no_drop"
            if edges:
                got = {(e["callee_path"], e.get("callee_name")) for e in edges}
                bound_to = sorted(got)[0]
                if bound_to[0] == dst_path:
                    tier = "misbind_name"
                elif os.path.dirname(bound_to[0]) == os.path.dirname(dst_path):
                    tier = "misbind_sibling_file"
                else:
                    tier = "misbind_other_crate"
        tiers[tier] += 1
        shape = classify(row, text, node, site, drop, owners.get(src_path, []), specifiers.get(src_path, []), corpus_types)
        classes[shape] += 1
        line_no = text.count("\n", 0, site["span"]["start"]) + 1 if site else 0
        if len(class_examples[shape]) < 4:
            class_examples[shape].append(f"{src_path}:{line_no} {caller} -> {dst_path} {callee}")
        out_rows.append(
            "\t".join([src_path, caller, dst_path, callee, tier, shape, str(line_no)])
        )

    print("tier\tcount")
    for tier, count in tiers.most_common():
        print(f"{tier}\t{count}")
    print("\nclass\tcount\texample")
    for shape, count in classes.most_common():
        print(f"{shape}\t{count}\t{class_examples[shape][0] if class_examples[shape] else ''}")
    print("\nclass examples")
    for shape, examples in class_examples.items():
        for example in examples[:2]:
            print(f"{shape}\t{example}")
    if args.out:
        with open(args.out, "w") as fh:
            fh.write("\n".join(out_rows) + "\n")


if __name__ == "__main__":
    main()
