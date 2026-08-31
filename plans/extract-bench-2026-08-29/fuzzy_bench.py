#!/usr/bin/env python3
"""Fuzzy re-measure of the extract bench under name-tolerant matching.

Lane fix-extract-bench-fuzzy (2026-08-31). The ratchet
(v6/sprefa-extract/tests/bench/mod.rs) joins ours-vs-oracle rows by exact
4-column equality; a row pointing at the right function with a different
name spelling scores as a miss. This script re-scores the SAME projected
row sets under three match modes:

  exact     4-column string equality (the ratchet's own scoring; the exact
            column below must equal RATCHET.tsv, else STOP)
  filepair  match on (src_path, dst_path) only, names ignored; an ours row
            matches if the oracle has ANY row with that file pair (name-blind
            upper bound)
  fuzzy     within a (src_path, dst_path) group, similarity = Jaccard over
            identifier tokens (split on :: . # / -> and camelCase, lowercase);
            greedy one-to-one assignment, highest similarity first; matched
            at >= threshold. Reported at 0.8 and 0.5.

Usage:
  fuzzy_bench.py <ours.tsv> <oracle.tsv> --lang LANG --oracle NAME [--kinds ours.kinds.tsv] [--files ours.files.tsv] [--mode exact|filepair|fuzzy] [--threshold 0.8]
  fuzzy_bench.py --all          # runs every (lang, oracle) pair of the brief

The ours tsvs are the ratchet's own emission, dumped by the lane's
temporary tests/fuzzy_dump.rs harness into out/ (gitignored): the raw
normal form of tests/bench/mod.rs, plus the call_kinds map and the corpus
file list the projections need.
"""

import argparse
import collections
import itertools
import re
import sys
from pathlib import Path

BENCH_DIR = Path(__file__).resolve().parent
OUT_DIR = BENCH_DIR / "out"

# The (lang, oracle) pairs and projections of the brief; RATCHET.tsv rows
# for these (recall, precision) are the exact-mode check.
PAIRS = [
    ("go", "go.codeql2.call.tsv"),
    ("go", "go.oracle.call.vta.bare.tsv"),
    ("rust", "rust.oracle.call.tsv"),
    ("rust", "rust.scip_override.call.tsv"),
    ("rust", "rust.codeql.call.tsv"),
    ("ts5", "ts.codeql2.call.tsv"),
    ("ts5", "ts5.oracle.call.tsv"),
]

# RATCHET.tsv exact-mode rows (ed6079f84) to reproduce to 0.01 pt.
RATCHET = {
    ("go", "go.codeql2.call.tsv"): (98.96, 90.78),
    ("go", "go.oracle.call.vta.bare.tsv"): (85.39, 81.60),
    ("rust", "rust.oracle.call.tsv"): (93.68, 55.98),
    ("rust", "rust.scip_override.call.tsv"): (77.89, 41.02),
    ("rust", "rust.codeql.call.tsv"): (73.36, 78.78),
    ("ts5", "ts.codeql2.call.tsv"): (92.07, 71.15),
    ("ts5", "ts5.oracle.call.tsv"): (88.20, 76.13),
}


def load_tsv(path):
    with open(path, encoding="utf-8") as handle:
        return {line.rstrip("\n") for line in handle if line.strip()}


def row_cols(row):
    parts = row.split("\t")
    while len(parts) < 4:
        parts.append("")
    return parts[:4]


# ---- projections, ported 1:1 from tests/bench/mod.rs ------------------------


def go_projection(ours_rows, kinds, oracle_rows, oracle_name):
    """go.project.py over a call set (tests/bench/mod.rs go_project)."""
    oracle_srcs = {row.split("\t")[0] for row in oracle_rows}
    scoped = [r for r in ours_rows if r.split("\t")[0] in oracle_srcs]
    scoped = [r for r in scoped if not r.split("\t")[1].startswith("closure@")]
    if oracle_name == "go.codeql2.call.tsv":
        # codeql shape: drop the per-implementer fan-out rows, keep the spec.
        return [r for r in scoped if kinds.get(r) != "implements"]
    if oracle_name == "go.oracle.call.vta.bare.tsv":
        # vta shape: keep fan-out rows; drop the spec row, detected as the
        # non-implements row whose (src_path, src_name, dst_name) triple also
        # occurs on an implements row.
        impl_triples = {
            tuple(r.split("\t")[i] for i in (0, 1, 3))
            for r in scoped
            if kinds.get(r) == "implements"
        }
        kept = []
        for row in scoped:
            parts = row.split("\t")
            if kinds.get(row) == "implements":
                kept.append(row)
            elif tuple(parts[i] for i in (0, 1, 3)) not in impl_triples:
                kept.append(row)
        return kept
    return scoped


def closure_enclosing(rows):
    # tests/bench/mod.rs closure_enclosing: a closure@<n> caller row drops
    # when a non-closure row shares its (src_path, dst_path, dst_name) triple.
    plain_triples = {
        (parts[0], parts[2], parts[3])
        for row in rows
        if not (parts := row.split("\t"))[1].startswith("closure@")
    }
    kept = []
    for row in rows:
        parts = row.split("\t")
        if not parts[1].startswith("closure@"):
            kept.append(row)
        elif (parts[0], parts[2], parts[3]) not in plain_triples:
            kept.append(row)
    return kept


def rust_projection(ours_rows, oracle_rows, corpus_files):
    # tests/bench/mod.rs rust_project: oracle rows drop when their dst_path
    # is outside the corpus; ours rows drop when their src_path is a file the
    # (dst-scoped) oracle never calls from; the closure mirror runs on both.
    oracle_scoped = [r for r in oracle_rows if r.split("\t")[2] in corpus_files]
    oracle_srcs = {row.split("\t")[0] for row in oracle_scoped}
    ours_scoped = [r for r in ours_rows if r.split("\t")[0] in oracle_srcs]
    return closure_enclosing(ours_scoped), closure_enclosing(oracle_scoped)


# ---- match modes ------------------------------------------------------------

TOKEN_SPLIT = re.compile(r"::|\.|#|/|->")


def tokens(name):
    # Split on :: . # / ->, then on camelCase boundaries, lowercase.
    name = TOKEN_SPLIT.sub(" ", name)
    spaced = re.sub(r"(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])", " ", name)
    return frozenset(tok.lower() for tok in spaced.split() if tok)


def jaccard(a, b):
    if not a or not b:
        return 0.0
    inter = len(a & b)
    if inter == 0:
        return 0.0
    return inter / (len(a) + len(b) - inter)


def pair_groups(rows):
    groups = collections.defaultdict(list)
    for row in rows:
        parts = row.split("\t")
        groups[(parts[0], parts[2])].append(row)
    return groups


def score_exact(ours, oracle):
    overlap = len(ours & oracle)
    return pct(overlap, len(oracle)), pct(overlap, len(ours))


def score_filepair(ours, oracle):
    ours_pairs = {(r.split("\t")[0], r.split("\t")[2]) for r in ours}
    oracle_pairs = {(r.split("\t")[0], r.split("\t")[2]) for r in oracle}
    # recall = oracle rows whose file pair ours also has; precision = ours
    # rows whose file pair the oracle also has.
    recall_hits = sum(1 for r in oracle if (r.split("\t")[0], r.split("\t")[2]) in ours_pairs)
    precision_hits = sum(1 for r in ours if (r.split("\t")[0], r.split("\t")[2]) in oracle_pairs)
    return pct(recall_hits, len(oracle)), pct(precision_hits, len(ours))


def match_fuzzy(ours, oracle, threshold):
    """Greedy one-to-one assignment per file-pair group, highest Jaccard of
    name tokens first. Returns (matched_ours_rows, matched_oracle_rows)."""
    ours_groups = pair_groups(sorted(ours))
    oracle_groups = pair_groups(sorted(oracle))
    matched_ours = set()
    matched_oracle = set()
    examples = []
    for key in ours_groups.keys() & oracle_groups.keys():
        candidates = []
        oracle_tokens = {row: tokens(row.split("\t")[3]) for row in oracle_groups[key]}
        for ours_row in ours_groups[key]:
            ours_tokens = tokens(ours_row.split("\t")[3])
            for oracle_row in oracle_groups[key]:
                sim = jaccard(ours_tokens, oracle_tokens[oracle_row])
                if sim >= threshold:
                    candidates.append((sim, ours_row, oracle_row))
        candidates.sort(key=lambda c: (-c[0], c[1], c[2]))
        taken_ours = set()
        taken_oracle = set()
        for sim, ours_row, oracle_row in candidates:
            if ours_row in taken_ours or oracle_row in taken_oracle:
                continue
            taken_ours.add(ours_row)
            taken_oracle.add(oracle_row)
            if sim < 1.0:
                examples.append((ours_row, oracle_row, sim))
        matched_ours |= taken_ours
        matched_oracle |= taken_oracle
    return matched_ours, matched_oracle, examples


def score_fuzzy(ours, oracle, threshold):
    matched_ours, matched_oracle, examples = match_fuzzy(ours, oracle, threshold)
    return pct(len(matched_oracle), len(oracle)), pct(len(matched_ours), len(ours)), examples


def pct(overlap, total):
    return 100.0 * overlap / total if total else 0.0


def run_pair(lang, oracle_name, verbose):
    ours_path = OUT_DIR / f"ours.{lang}.call.tsv"
    kinds_path = OUT_DIR / f"ours.{lang}.kinds.tsv"
    files_path = OUT_DIR / f"ours.{lang}.files.tsv"
    oracle_path = BENCH_DIR / oracle_name

    ours_rows = sorted(load_tsv(ours_path))
    oracle_rows_all = sorted(load_tsv(oracle_path))
    kinds = {}
    if lang == "go":
        with open(kinds_path, encoding="utf-8") as handle:
            for line in handle:
                row, _, kind = line.rstrip("\n").rpartition("\t")
                kinds[row] = kind
    corpus_files = set()
    if lang == "rust":
        with open(files_path, encoding="utf-8") as handle:
            corpus_files = {line.strip() for line in handle if line.strip()}

    if lang == "rust":
        oracle_scoped = closure_enclosing([r for r in oracle_rows_all if r.split("\t")[2] in corpus_files])
        oracle_srcs = {r.split("\t")[0] for r in oracle_scoped}
        ours_proj = set(closure_enclosing([r for r in ours_rows if r.split("\t")[0] in oracle_srcs]))
        oracle_proj = set(oracle_scoped)
    elif lang == "go":
        ours_proj = set(go_projection(ours_rows, kinds, oracle_rows_all, oracle_name))
        oracle_proj = set(oracle_rows_all)
    else:
        ours_proj = set(ours_rows)
        oracle_proj = set(oracle_rows_all)

    rows = []
    exact_r, exact_p = score_exact(ours_proj, oracle_proj)
    floor = RATCHET[(lang, oracle_name)]
    exact_ok = abs(exact_r - floor[0]) <= 0.01 and abs(exact_p - floor[1]) <= 0.01
    if not exact_ok and verbose:
        print(f"STOP CHECK {lang} {oracle_name}: exact recall {exact_r:.2f}/{floor[0]:.2f} "
              f"precision {exact_p:.2f}/{floor[1]:.2f}", file=sys.stderr)
    rows.append((lang, oracle_name, "exact", "", exact_r, exact_p, len(ours_proj), len(oracle_proj), len(ours_proj & oracle_proj)))

    fp_r, fp_p = score_filepair(ours_proj, oracle_proj)
    rows.append((lang, oracle_name, "filepair", "", fp_r, fp_p, len(ours_proj), len(oracle_proj), None))

    for threshold in (0.8, 0.5):
        f_r, f_p, examples = score_fuzzy(ours_proj, oracle_proj, threshold)
        rows.append((lang, oracle_name, "fuzzy", f"{threshold:.1f}", f_r, f_p, len(ours_proj), len(oracle_proj), None))

    return rows, (ours_proj, oracle_proj), exact_ok


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ours", nargs="?")
    parser.add_argument("oracle", nargs="?")
    parser.add_argument("--lang")
    parser.add_argument("--oracle-name")
    parser.add_argument("--kinds")
    parser.add_argument("--files")
    parser.add_argument("--mode", default="fuzzy", choices=["exact", "filepair", "fuzzy"])
    parser.add_argument("--threshold", type=float, default=0.8)
    parser.add_argument("--all", action="store_true", help="run every brief pair")
    parser.add_argument("--examples", type=int, default=0, help="print N fuzzy-matched miss-at-exact example rows")
    args = parser.parse_args()

    if args.all:
        all_rows = []
        ok = True
        for lang, oracle_name in PAIRS:
            rows, (ours, oracle), exact_ok = run_pair(lang, oracle_name, verbose=True)
            ok = ok and exact_ok
            all_rows.extend(rows)
            if args.examples and lang == "rust" and oracle_name == "rust.oracle.call.tsv":
                matched_ours, _, examples = match_fuzzy(ours, oracle, 0.8)
                exact_matched = ours & oracle
                shown = 0
                for ours_row, oracle_row, sim in sorted(examples, key=lambda e: -e[2]):
                    if ours_row in exact_matched:
                        continue
                    print(f"EXAMPLE\t{sim:.3f}\tours: {ours_row}\toracle: {oracle_row}", file=sys.stderr)
                    shown += 1
                    if shown >= args.examples:
                        break
        header = "lang\toracle\tmode\tthreshold\trecall\tprecision\tours\toracle\toverlap"
        print(header)
        for row in all_rows:
            print("\t".join("" if c is None else str(c) for c in row))
        sys.exit(0 if ok else 2)

    if not (args.ours and args.oracle and args.lang and args.oracle_name):
        parser.error("single-pair mode needs OURS.tsv ORACLE.tsv --lang --oracle-name")
    ours_rows = sorted(load_tsv(args.ours))
    oracle_rows = sorted(load_tsv(args.oracle))
    kinds = {}
    if args.kinds:
        with open(args.kinds, encoding="utf-8") as handle:
            for line in handle:
                row, _, kind = line.rstrip("\n").rpartition("\t")
                kinds[row] = kind
    corpus_files = set()
    if args.files:
        with open(args.files, encoding="utf-8") as handle:
            corpus_files = {line.strip() for line in handle if line.strip()}
    if args.lang == "rust":
        oracle_scoped = closure_enclosing([r for r in oracle_rows if r.split("\t")[2] in corpus_files])
        oracle_srcs = {r.split("\t")[0] for r in oracle_scoped}
        ours_proj = set(closure_enclosing([r for r in ours_rows if r.split("\t")[0] in oracle_srcs]))
        oracle_proj = set(oracle_scoped)
    elif args.lang == "go":
        ours_proj = set(go_projection(ours_rows, kinds, oracle_rows, args.oracle_name))
        oracle_proj = set(oracle_rows)
    else:
        ours_proj = set(ours_rows)
        oracle_proj = set(oracle_rows)

    if args.mode == "exact":
        recall, precision = score_exact(ours_proj, oracle_proj)
    elif args.mode == "filepair":
        recall, precision = score_filepair(ours_proj, oracle_proj)
    else:
        recall, precision, _ = score_fuzzy(ours_proj, oracle_proj, args.threshold)
    print(f"{args.mode}\t{args.threshold:g}\trecall {recall:.2f}\tprecision {precision:.2f}")


if __name__ == "__main__":
    main()
