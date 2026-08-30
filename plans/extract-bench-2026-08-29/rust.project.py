#!/usr/bin/env python3
"""The rust call projection: one normal form applied to our rows and to the
scip oracle so recall and precision against ra_ap_ide and raw scip are
comparable (RUST-PARITY.REPORT.md).

Two flags do real work on this corpus:
  --scope corpus    drop oracle rows whose dst_path is outside the corpus
                    file list; drop our rows whose src_path is not in the
                    oracle's src_path set.
  --closure enclosing
                    drop `closure@<n>` src_name rows when a mirrored
                    enclosing-fn row exists (same src_path, dst_path,
                    dst_name, with a src_name that is not a closure). The
                    ra_ap_ide oracle has no closure rows; raw scip does.

  --generic         strip turbofish/generic suffixes (`a::b::<T, U>` and
                    `name::<T>`) from all name columns. A no-op on this
                    corpus: no rust*.call.tsv row carries a `<` (grep -c '<'
                    = 0 on oracle, scip and ours at sha d1ebd8c42).

Oracle-vs-oracle mode: pass two oracles as --ours/--oracle, both get the
scope and closure legs.

usage: rust.project.py --ours <tsv> --oracle <tsv>
                       [--corpus-root DIR | --files <txt>]
                       [--scope corpus] [--closure enclosing] [--generic]
                       [--out-ours f] [--out-oracle f]
Prints |ours| |oracle| overlap recall precision.
"""
import argparse
import os
import sys


def load_rows(path):
    rows = set()
    with open(path) as fh:
        for line in fh:
            line = line.rstrip("\n")
            if line:
                rows.add(line)
    return rows


def corpus_files(root):
    # The ratchet's file rule: every .rs under crates/ whose path carries a
    # `src` component (tests/bench/mod.rs, `wants`).
    found = []
    for dirpath, dirnames, filenames in os.walk(os.path.join(root, "crates")):
        dirnames[:] = [d for d in dirnames if d != "target"]
        for name in filenames:
            if not name.endswith(".rs"):
                continue
            rel = os.path.relpath(os.path.join(dirpath, name), root)
            if any(part == "src" for part in rel.split(os.sep)[:-1]):
                found.append(rel)
    return set(found)


def strip_generic(name):
    # Turbofish and generic suffixes: `collect::<Vec<_>>()` resolves a callee
    # name carrying `<...>`; peel from the first `<` outside the path (names
    # only; the caller splits columns before this sees them).
    depth = 0
    for i, ch in enumerate(name):
        if ch == "<" and depth == 0:
            return name[:i]
        if ch == "<":
            depth += 1
        elif ch == ">":
            depth -= 1
    return name


def row_cols(row):
    cols = row.split("\t")
    while len(cols) < 4:
        cols.append("")
    return cols


def closure_enclosing(rows):
    # Drop a `closure@<n>` caller row when a non-closure row shares the same
    # (src_path, dst_path, dst_name) triple: the enclosing-fn mirror exists,
    # so the closure row is a duplicate site.
    cols_of = [row_cols(row) for row in rows]
    plain = {
        (c[0], c[2], c[3])
        for c in cols_of
        if not c[1].startswith("closure@")
    }
    return {
        row for row, c in zip(rows, cols_of)
        if not c[1].startswith("closure@")
        or (c[0], c[2], c[3]) not in plain
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ours", required=True)
    ap.add_argument("--oracle", required=True)
    ap.add_argument("--corpus-root")
    ap.add_argument("--files")
    ap.add_argument("--scope", choices=["corpus"])
    ap.add_argument("--closure", choices=["enclosing"])
    ap.add_argument("--generic", action="store_true")
    ap.add_argument("--out-ours")
    ap.add_argument("--out-oracle")
    args = ap.parse_args()

    ours = load_rows(args.ours)
    oracle = load_rows(args.oracle)

    if args.generic:
        def degen(row):
            cols = row_cols(row)
            cols[1] = strip_generic(cols[1])
            cols[3] = strip_generic(cols[3])
            return "\t".join(cols)
        ours = {degen(row) for row in ours}
        oracle = {degen(row) for row in oracle}

    if args.scope == "corpus":
        if args.corpus_root:
            files = corpus_files(args.corpus_root)
        elif args.files:
            with open(args.files) as fh:
                files = {line.strip() for line in fh if line.strip()}
        else:
            sys.exit("--scope corpus needs --corpus-root or --files")
        # Oracle side: the callee must be a corpus file.
        oracle = {
            row for row in oracle if row_cols(row)[2] in files
        }
        # Our side: the caller must be a file the oracle itself calls from.
        oracle_srcs = {row_cols(row)[0] for row in oracle}
        ours = {row for row in ours if row_cols(row)[0] in oracle_srcs}

    if args.closure == "enclosing":
        cols_of = [row_cols(row) for row in ours]
        ours = closure_enclosing(ours)
        oracle = closure_enclosing(oracle)

    overlap = len(ours & oracle)
    recall = overlap * 100.0 / len(oracle) if oracle else 0.0
    precision = overlap * 100.0 / len(ours) if ours else 0.0
    print(f"{len(ours)}\t{len(oracle)}\t{overlap}\t{recall:.2f}\t{precision:.2f}")

    for path, rows in ((args.out_ours, ours), (args.out_oracle, oracle)):
        if path:
            with open(path, "w") as f:
                f.write("\n".join(sorted(rows)) + "\n")


if __name__ == "__main__":
    main()
