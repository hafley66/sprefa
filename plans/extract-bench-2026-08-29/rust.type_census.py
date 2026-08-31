#!/usr/bin/env python3
"""Classify the rust type leg's missing oracle rows by syntactic shape.

The join rust.REPORT.md sec 26.1 describes, made reproducible. Three inputs:

  ours    RUST_TYPE_DUMP=<path> from tests/79_rust_type_dump.rs
  census  SHAPE_CENSUS=<path> from the same test (syn walk, owner_of port)
  oracle  rust.oracle.type.typedecl.tsv

`ours` and `oracle` are 4-col (src_file, owner, dst_file, dst_name); `census`
is 6-col (file, owner, dst, root, leaf, qualified). A missing oracle row joins
to the census on (src_file, owner, dst_name); the (root, leaf) positions it
occupies name the class.

    python3 rust.type_census.py <ours> <census> [--oracle F] [--root D]
                               [--examples N] [--class C]
"""

import argparse
import collections
import os
import pathlib
import random
import sys

BENCH = pathlib.Path(__file__).resolve().parent
DEFAULT_ORACLE = BENCH / "rust.oracle.type.typedecl.tsv"
DEFAULT_ROOT = "/Users/chrishafley/projects/rust-analyzer"


def read_rows(path, cols):
    out = []
    with open(path) as handle:
        for line in handle:
            line = line.rstrip("\n")
            if not line:
                continue
            parts = line.split("\t")
            parts += [""] * (cols - len(parts))
            out.append(tuple(parts[:cols]))
    return out


def corpus_files(root):
    """tests/bench/mod.rs `wants`: crates/*/**/src/**/*.rs."""
    found = set()
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [
            d for d in dirnames
            if not d.startswith(".") and d not in ("target", "node_modules")
        ]
        for name in filenames:
            if not name.endswith(".rs"):
                continue
            rel = os.path.relpath(os.path.join(dirpath, name), root)
            parts = rel.split("/")
            if parts[0] == "crates" and len(parts) > 2 and "src" in parts[1:]:
                found.add(rel)
    return found


ARG_LEAVES = {
    "generic-arg", "assoc-binding", "assoc-constraint",
    "fn-trait-arg", "fn-trait-ret", "tuple-elem", "path-prefix",
}


def from_positions(hits):
    """The class a set of (root, leaf) census positions names."""
    roots = {root for root, _leaf in hits}
    leaves = {leaf for _root, leaf in hits}
    if "variant-payload" in roots:
        return "B enum variant payload"
    if "alias-rhs" in roots:
        return "A alias rhs"
    if "assoc-type" in roots:
        return "A1 assoc-type body"
    if "impl-self-ty" in roots:
        return "D2 impl self type"
    if "impl-trait" in roots:
        return "E1 implements"
    if roots & {"bound", "supertrait", "where-bounded-ty", "generic-param-default"}:
        return "D1 bound generic args" if leaves & ARG_LEAVES else "F bound head"
    if "field" in roots:
        return "K field"
    return None


# Warranted exclusions, then the file-local join, then the cross-file join (an
# impl block sits in a file the oracle does not key the row on), then residual.
def classify(row, by_key, by_pair, owners, files):
    src, owner, dst_file, dst = row
    if src not in files:
        return "X1 outside the corpus"
    if owner == dst:
        return "X2 self-edge, same file" if src == dst_file else "X2b self-edge, cross-file"
    hit = from_positions(by_key.get((src, owner, dst), ()))
    if hit:
        return hit
    elsewhere = by_pair.get((owner, dst))
    if elsewhere:
        hit = from_positions({(root, leaf) for _file, root, leaf in elsewhere})
        if hit:
            return hit + ", declared in another file"
        return "J unexplained"
    if owner not in owners:
        return "X3 owner is a path prefix"
    return "J unexplained"


def locate(root, rel, owner, dst, cache):
    """The 1-indexed line the example row is readable at: the first line naming
    both owner and dst, else the first naming dst."""
    if rel not in cache:
        try:
            cache[rel] = open(os.path.join(root, rel)).read().splitlines()
        except OSError:
            cache[rel] = []
    fallback = 0
    for num, line in enumerate(cache[rel], 1):
        if dst not in line:
            continue
        if owner in line:
            return num
        fallback = fallback or num
    return fallback


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ours")
    ap.add_argument("census")
    ap.add_argument("--oracle", default=str(DEFAULT_ORACLE))
    ap.add_argument("--root", default=DEFAULT_ROOT)
    ap.add_argument("--examples", type=int, default=5)
    ap.add_argument("--class", dest="only", default=None)
    ap.add_argument("--seed", type=int, default=7)
    args = ap.parse_args()

    ours = set(read_rows(args.ours, 4))
    oracle = set(read_rows(args.oracle, 4))
    census = read_rows(args.census, 6)
    files = corpus_files(args.root)

    census_by_key = collections.defaultdict(set)
    census_by_pair = collections.defaultdict(set)
    census_owners = set()
    for file, owner, dst, root, leaf, _qualified in census:
        census_by_key[(file, owner, dst)].add((root, leaf))
        census_by_pair[(owner, dst)].add((file, root, leaf))
        census_owners.add(owner)

    overlap = ours & oracle
    missing = oracle - ours
    excess = ours - oracle
    recall = 100.0 * len(overlap) / len(oracle) if oracle else 0.0
    precision = 100.0 * len(overlap) / len(ours) if ours else 0.0

    print(f"oracle {len(oracle)}  ours {len(ours)}  overlap {len(overlap)}  "
          f"missing {len(missing)}  excess {len(excess)}")
    print(f"recall {recall:.2f}  precision {precision:.2f}")
    print(f"corpus files {len(files)}  census rows {len(census)}\n")

    buckets = collections.defaultdict(list)
    for row in sorted(missing):
        cls = classify(row, census_by_key, census_by_pair, census_owners, files)
        buckets[cls].append(row)

    print(f"{'class':<32} {'rows':>6} {'% oracle':>9}")
    for name, rows in sorted(buckets.items(), key=lambda kv: -len(kv[1])):
        print(f"{name:<32} {len(rows):>6} {100.0 * len(rows) / len(oracle):>8.2f}%")

    rng = random.Random(args.seed)
    cache = {}
    for name, rows in sorted(buckets.items(), key=lambda kv: -len(kv[1])):
        if args.only and not name.startswith(args.only):
            continue
        print(f"\n== {name} ({len(rows)} rows)")
        for src, owner, dst_file, dst in rng.sample(rows, min(args.examples, len(rows))):
            positions = sorted(census_by_key.get((src, owner, dst), []))
            line = locate(args.root, src, owner, dst, cache)
            print(f"   {src}:{line}  {owner} -> {dst}  [{dst_file}]  {positions}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
