#!/usr/bin/env python3
"""Classify the rust call EXCESS: rows we emit that an oracle does not.

Input mirrors `rust.leak.classify.py` (--percall, --resolve, --root). Each
excess row is joined back to the `resolved_edge` rows that produced it, so the
class carries the LEG that minted it (`kind`) plus the shape of the site.

Tiers, in decision order:
  no_def_in_dst   the dst file declares no def of that name (a pure name pick)
  ctor_target     the dst def is a type or a variant, not a fn
  <kind>          the resolving leg, when the dst def is a real fn

usage: rust.excess2.classify.py --excess f --percall f --resolve f --root DIR
                                [--sample N --seed S] [--out f]
"""
import argparse
import json
import os
import random
from collections import Counter, defaultdict


def load_percall(path):
    nodes = defaultdict(list)
    sites = defaultdict(list)
    owners = defaultdict(list)
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            rel = row["__f"]
            if row["record"] == "node":
                nodes[rel].append(row)
            elif row["record"] == "site":
                sites[rel].append(row)
            elif row["record"] == "method_owner":
                owners[rel].append(row)
    return nodes, sites, owners


def load_edges(path):
    by_row = defaultdict(list)
    with open(path) as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            if row["record"] != "resolved_edge":
                continue
            key = (
                row["caller_path"],
                row.get("caller_name") or "",
                row["callee_path"],
                row.get("callee_name") or "",
            )
            by_row[key].append(row)
    return by_row


def receiver_text(text, site_start):
    if site_start <= 0 or text[site_start - 1 : site_start] != ".":
        return ""
    i = site_start - 1
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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--excess", required=True)
    ap.add_argument("--percall", required=True)
    ap.add_argument("--resolve", required=True)
    ap.add_argument("--root", required=True)
    ap.add_argument("--sample", type=int, default=0)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--out")
    args = ap.parse_args()

    nodes, sites, owners = load_percall(args.percall)
    edges = load_edges(args.resolve)

    fn_kinds = {"function", "method"}
    dst_defs = {}
    for rel, rows in nodes.items():
        table = defaultdict(set)
        for row in rows:
            if row.get("name"):
                table[row["name"]].add(row["kind"])
        dst_defs[rel] = table

    rows = [line.split("\t") for line in open(args.excess).read().splitlines() if line]
    rows = [row for row in rows if len(row) == 4]
    if args.sample:
        random.Random(args.seed).shuffle(rows)
        rows = rows[: args.sample]

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
    shapes = Counter()
    examples = defaultdict(list)
    out_rows = []
    for src_path, caller, dst_path, callee in rows:
        found = edges.get((src_path, caller, dst_path, callee), [])
        kinds = {row["kind"] for row in found}
        starts = [row["caller_site_start"] for row in found if row["caller_site_start"]]
        declared = dst_defs.get(dst_path, {}).get(callee, set())
        if not declared:
            tier = "no_def_in_dst"
        elif not (declared & fn_kinds):
            tier = "ctor_target"
        else:
            tier = "leg:" + (sorted(kinds)[0] if kinds else "unknown")
        text = source(src_path)
        start = starts[0] if starts else 0
        recv = receiver_text(text, start)
        line_no = text.count("\n", 0, start) + 1 if start else 0
        if recv.startswith("self."):
            shape = "receiver_own_field"
        elif recv in ("self",):
            shape = "receiver_self"
        elif recv:
            shape = "receiver_local"
        elif os.path.dirname(src_path) == os.path.dirname(dst_path):
            shape = "bare_name_sibling_file"
        elif src_path.split("/")[1] == dst_path.split("/")[1]:
            shape = "bare_name_same_crate"
        else:
            shape = "bare_name_other_crate"
        key = f"{tier}/{shape}"
        tiers[tier] += 1
        shapes[key] += 1
        if len(examples[key]) < 3:
            examples[key].append(f"{src_path}:{line_no} {caller} -> {dst_path} {callee}")
        out_rows.append("\t".join([src_path, caller, dst_path, callee, tier, shape, str(line_no)]))

    print("tier\tcount")
    for tier, count in tiers.most_common():
        print(f"{tier}\t{count}")
    print("\ntier/shape\tcount\texample")
    for key, count in shapes.most_common(20):
        print(f"{key}\t{count}\t{examples[key][0] if examples[key] else ''}")
    if args.out:
        with open(args.out, "w") as fh:
            fh.write("\n".join(out_rows) + "\n")


if __name__ == "__main__":
    main()
