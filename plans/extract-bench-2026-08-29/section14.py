#!/usr/bin/env python3
"""Emit the section-14 markdown tables: every ours-column row recomputed
against every oracle and tool tsv present, chunked beside single process.

recall = overlap / |oracle|, precision = overlap / |ours|. That is section
13's convention, NOT bench.py's, whose two labels are the reverse pair.
"""
import os
import sys

LAB = os.path.dirname(os.path.abspath(__file__))
CTL = "/tmp/spctl"

PAIRS = {
    "go": [
        ("call", "go.parse.call.chunked.tsv", "go.parse.call.tsv", "go.ctl.call.tsv", [
            "go.oracle.call.vta.bare.tsv", "go.oracle.call.cha.tsv",
            "go.codeql2.call.tsv", "go.joern2.call.tsv"]),
        ("type", "go.parse.type.chunked.tsv", "go.parse.type.tsv", "go.ctl.type.tsv", [
            "go.oracle.type.tsv", "go.oracle.type.typedecl.tsv"]),
        ("module", "go.parse.module.chunked.tsv", "go.parse.module.tsv", "go.ctl.module.tsv", [
            "go.oracle.module.tsv"]),
    ],
    "ts5": [
        ("call", "ts5.parse.call.chunked.tsv", "ts5.parse.call.tsv", "ts5.ctl.call.tsv", [
            "ts5.oracle.call.tsv", "ts.codeql2.call.tsv", "ts.joern2.call.tsv"]),
        ("type", "ts5.parse.type.chunked.tsv", "ts5.parse.type.tsv", "ts5.ctl.type.tsv", []),
        ("module", "ts5.parse.module.chunked.tsv", "ts5.parse.module.tsv", "ts5.ctl.module.tsv", [
            "ts5.oracle.module.tsv", "ts.madge.module.tsv", "ts.depcruise.module.tsv",
            "ts.stackgraphs.module.tsv", "ts.codeql.module.tsv"]),
    ],
    "rust": [
        ("call", "rust.parse.call.chunked.tsv", "rust.parse.call.tsv", "rust.ctl.call.tsv", [
            "rust.oracle.call.tsv"]),
        ("type", "rust.parse.type.chunked.tsv", "rust.parse.type.tsv", "rust.ctl.type.tsv", [
            "rust.oracle.type.tsv", "rust.oracle.type.typedecl.tsv"]),
        ("module", None, "rust.parse.module.tsv", "rust.ctl.module.tsv", []),
    ],
}

WALL_RSS = {
    ("go", "resolve"): ("9.38", "666"),
    ("ts5", "resolve"): ("2.48", "408"),
    ("rust", "resolve"): ("2.18", "536"),
}


def load(path, base=LAB):
    if path is None:
        return None
    full = path if os.path.isabs(path) else os.path.join(base, path)
    if not os.path.exists(full):
        return None
    with open(full) as fh:
        return {line.rstrip("\n") for line in fh if line.strip()}


def pct(num, den):
    return f"{100.0 * num / den:.1f}%" if den else "n/a"


def cell(ours, oracle):
    if ours is None:
        return "n/a", "n/a"
    inter = ours & oracle
    return pct(len(inter), len(oracle)), pct(len(inter), len(ours))


def main():
    for lang in ("go", "ts5", "rust"):
        wall, rss = WALL_RSS[(lang, "resolve")]
        print(f"\n#### {lang}, one process, {wall} s wall, {rss} MB peak RSS\n")
        print("| family | oracle / tool | oracle rows | ours chunked | chunked recall | "
              "chunked precision | ours single | single recall | single precision |")
        print("|---|---|---:|---:|---:|---:|---:|---:|---:|")
        for family, _committed_p, single_p, ctl_p, oracles in PAIRS[lang]:
            chunked = load(ctl_p, CTL)
            single = load(single_p)
            if not oracles:
                print(f"| {family} | no oracle in this lab | n/a | "
                      f"{len(chunked) if chunked else 'n/a'} | n/a | n/a | "
                      f"{len(single)} | n/a | n/a |")
                continue
            for oracle_p in oracles:
                oracle = load(oracle_p)
                if oracle is None:
                    continue
                c_rec, c_pre = cell(chunked, oracle)
                s_rec, s_pre = cell(single, oracle)
                print(f"| {family} | `{oracle_p}` | {len(oracle)} | "
                      f"{len(chunked) if chunked else 'n/a'} | {c_rec} | {c_pre} | "
                      f"{len(single)} | {s_rec} | {s_pre} |")

    print("\n#### the committed `*.chunked.tsv` files, against the same oracles\n")
    print("| lang | family | committed chunked rows | oracle | recall | precision |")
    print("|---|---|---:|---|---:|---:|")
    for lang in ("go", "ts5", "rust"):
        for family, committed_p, _s, _c, oracles in PAIRS[lang]:
            committed = load(committed_p)
            if committed is None:
                continue
            for oracle_p in oracles:
                oracle = load(oracle_p)
                if oracle is None:
                    continue
                rec, pre = cell(committed, oracle)
                print(f"| {lang} | {family} | {len(committed)} | `{oracle_p}` | {rec} | {pre} |")


if __name__ == "__main__":
    main()
