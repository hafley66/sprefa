#!/usr/bin/env python3
"""The go call projection: one normal form applied to our rows so recall and
precision against codeql2 and vta are comparable (GO-PARITY.REPORT.md).

Oracle rows are never touched: codeql2 already names the interface method and
vta already names implementers, and neither contains a _test.go row. Every
flag applies to OURS only. Stdlib only.

usage: go.project.py --ours <tsv> [--kinds <tsv>] [--scope-oracle <tsv>]
                     [--closure] [--iface method|impl|both] [--out <tsv>]
Prints |ours| (and writes the projected set with --out).

kinds tsv = the call tsv plus a 5th `kind` column (out/go.ours.call.kinds.tsv);
required only when --iface is used, because `implements`-kind rows are the
per-implementer fan-out edges (CallEdgeKind::Implements, go.rs) and the
spec edge (I.M) is what codeql keeps.
"""
import argparse
import sys


def load_rows(path):
    rows = []
    with open(path) as fh:
        for line in fh:
            line = line.rstrip("\n")
            if line:
                rows.append(line)
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ours", required=True)
    ap.add_argument("--kinds")
    ap.add_argument("--scope-oracle")
    ap.add_argument("--closure", action="store_true")
    ap.add_argument("--iface", choices=["method", "impl", "both"])
    ap.add_argument("--out")
    args = ap.parse_args()

    rows = set(load_rows(args.ours))

    if args.scope_oracle:
        oracle_srcs = {row.split("\t")[0] for row in load_rows(args.scope_oracle)}
        rows = {row for row in rows if row.split("\t")[0] in oracle_srcs}

    if args.closure:
        rows = {row for row in rows if not row.split("\t")[1].startswith("closure@")}

    if args.iface and args.iface != "both":
        if not args.kinds:
            sys.exit("--iface needs --kinds (the kind column marks implements rows)")
        kinds = {}
        for line in load_rows(args.kinds):
            row, kind = line.rsplit("\t", 1)
            kinds[row] = kind
        impl_rows = {row for row in rows if kinds.get(row) == "implements"}
        if args.iface == "method":
            # codeql shape: the interface method row, no per-implementer rows.
            rows = rows - impl_rows
        else:
            # vta shape: per-implementer rows; drop the I.M spec row, which we
            # detect as the non-implements row whose (src_path, src_name,
            # dst_name) triple also occurs on an implements row.
            impl_triples = {
                tuple(row.split("\t")[i] for i in (0, 1, 3)) for row in impl_rows
            }
            rows = rows - {
                row
                for row in rows
                if kinds.get(row) != "implements"
                and tuple(row.split("\t")[i] for i in (0, 1, 3)) in impl_triples
            }

    if args.out:
        with open(args.out, "w") as f:
            f.write("\n".join(sorted(rows)) + "\n")
    print(len(rows))


if __name__ == "__main__":
    main()
