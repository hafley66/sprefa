#!/usr/bin/env python3
"""Run our python arm (extract --resolve) over every PyCG micro-benchmark
case, score exact + fuzzy per case, aggregate per category.

Usage:
  pycg_score.py [--suite SUITE_DIR] [--extract BIN] [--out DIR] [--oracle pycg|trace]

`--oracle trace` reads `python-oracle/trace/TRACE.tsv` (written by
`python-oracle/trace/run.py`) in place of the PyCG callgraph.json rows and
writes `python-oracle/trace/SCORES.tsv` + `MISSES.tsv`. A trace holds only
executed edges, so it scores recall-of-covered (ours & trace / trace) and the
3-bucket split of ours (matched / contradicted / unjudged, keyed on the
caller columns as `tests/bench/mod.rs` `buckets` does); flat precision is
not a trace verdict.

Outputs in --out (default: alongside this file, python-oracle/):
  oracle/<category>_<case>.call.tsv   converted PyCG oracle rows
  ours/<category>_<case>.call.tsv     our resolved_edge rows
  SCORES.tsv                          per-case rows
Per case the scorer reports:
  oracle edges    non-external oracle rows (recall denominator)
  external        oracle rows whose callee is <builtin>/py-stdlib/builtin-type
  ours edges      our deduped resolved_edge rows
  recall/precision under exact 4-column match and fuzzy 0.8
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent
SUITE_DEFAULT = HERE / "python-oracle" / "suite"
EXTERNAL = "<external>"

sys.path.insert(0, str(BENCH))
import pycg_convert  # noqa: E402


def run_ours(extract, suite, case_dir, tmpdir, name):
    py_files = sorted(str(p.relative_to(suite)) for p in case_dir.rglob("*.py"))
    # extract --resolve needs two or more paths; a single-file case repeats
    # its one file, which yields the same single stream.
    paths = py_files if len(py_files) > 1 else py_files + py_files
    out = subprocess.run(
        [extract, "--resolve", *paths],
        cwd=suite,
        capture_output=True,
        text=True,
        timeout=10,
    )
    rows = set()
    for line in out.stdout.splitlines():
        if not line.strip():
            continue
        rec = json.loads(line)
        if rec.get("record") != "resolved_edge":
            continue
        caller = rec["caller_path"] or ""
        callee = rec["callee_path"] or ""
        cn = rec["caller_name"] or ""
        dn = rec["callee_name"] or ""
        rows.add(f"{caller}\t{cn}\t{callee}\t{dn}")
    if out.returncode != 0:
        print(f"WARN {name}: extract exit {out.returncode}: {out.stderr.strip()[:200]}", file=sys.stderr)
    return sorted(rows)


def load_tsv(path):
    return {line.rstrip("\n") for line in path.read_text().splitlines() if line.strip()}


def pct(hits, total):
    return 100.0 * hits / total if total else 0.0


def fuzzy_score(ours, oracle, threshold=0.8):
    import fuzzy_bench

    mo, mr, _ = fuzzy_bench.match_fuzzy(set(ours), set(oracle), threshold)
    return pct(len(mr), len(oracle)), pct(len(mo), len(ours))


def buckets(ours, oracle):
    """3-bucket split of ours keyed on (src_path, src_name): matched,
    contradicted (the oracle spoke about that caller, with other callees),
    unjudged (the oracle never mentions the caller)."""
    judged = {tuple(row.split("\t")[:2]) for row in oracle}
    found = {"matched": 0, "contradicted": 0, "unjudged": 0}
    for row in ours:
        if row in oracle:
            found["matched"] += 1
        elif tuple(row.split("\t")[:2]) in judged:
            found["contradicted"] += 1
        else:
            found["unjudged"] += 1
    return found


def load_trace(trace_dir):
    """TRACE.tsv -> {case: set(4-col rows)}; RUNS.tsv -> {case: (status, detail)}."""
    rows = {}
    for line in (trace_dir / "TRACE.tsv").read_text().splitlines()[1:]:
        if not line.strip():
            continue
        case, _category, *cols = line.split("\t")
        rows.setdefault(case, set()).add("\t".join(cols))
    runs = {}
    runs_path = trace_dir / "RUNS.tsv"
    if runs_path.exists():
        for line in runs_path.read_text().splitlines()[1:]:
            if not line.strip():
                continue
            case, _category, status, detail, *_rest = line.split("\t")
            runs[case] = (status, detail)
    return rows, runs


TRACE_COLUMNS = (
    "case\tcategory\tstatus\tpycg_edges\ttrace_edges\ttrace_and_pycg\tours_edges"
    "\tours_and_trace\trecall_of_covered_pct\tmatched\tcontradicted\tunjudged"
)


def score_trace(args, suite, out_dir, cases):
    """Score ours against the executed edges of python-oracle/trace/TRACE.tsv."""
    trace_dir = Path(args.trace) if args.trace else out_dir / "trace"
    oracle_dir = out_dir / "oracle"
    ours_dir = out_dir / "ours"
    ours_dir.mkdir(parents=True, exist_ok=True)
    trace_rows, runs = load_trace(trace_dir)
    per_case = []
    for case_dir in cases:
        prefix = f"{case_dir.parent.name}/{case_dir.name}"
        name = prefix.replace("/", "__")
        pycg_path = oracle_dir / f"{name}.call.tsv"
        pycg_all = load_tsv(pycg_path) if pycg_path.exists() else set()
        pycg = {r for r in pycg_all if r.split("\t")[2] != f"{prefix}/{EXTERNAL}"}
        trace = trace_rows.get(prefix, set())
        try:
            ours_rows = run_ours(args.extract, suite, case_dir, ours_dir, name)
        except subprocess.TimeoutExpired:
            print(f"TIMEOUT {name}", file=sys.stderr)
            ours_rows = []
        (ours_dir / f"{name}.call.tsv").write_text("\n".join(ours_rows) + ("\n" if ours_rows else ""))
        ours = set(ours_rows)
        status, _detail = runs.get(prefix, ("unknown", ""))
        per_case.append(
            {
                "case": prefix,
                "category": case_dir.parent.name,
                "status": status,
                "pycg": len(pycg),
                "trace": len(trace),
                "trace_and_pycg": len(trace & pycg),
                "ours": len(ours),
                "ours_and_trace": len(ours & trace),
                "buckets": buckets(ours, trace),
                "misses": sorted(trace - ours),
                "excess": sorted(ours - trace),
            }
        )

    sums = ("pycg", "trace", "trace_and_pycg", "ours", "ours_and_trace")
    bucket_keys = ("matched", "contradicted", "unjudged")

    def row_of(label, category, status, k):
        return (
            f"{label}\t{category}\t{status}\t{k['pycg']}\t{k['trace']}\t{k['trace_and_pycg']}\t{k['ours']}"
            f"\t{k['ours_and_trace']}\t{pct(k['ours_and_trace'], k['trace']):.2f}"
            f"\t{k['buckets']['matched']}\t{k['buckets']['contradicted']}\t{k['buckets']['unjudged']}"
        )

    def fold(acc, c):
        for f in sums:
            acc[f] += c[f]
        for f in bucket_keys:
            acc["buckets"][f] += c["buckets"][f]
        acc["cases"] += 1
        acc["ok"] += c["status"] == "ok"

    def empty():
        return {f: 0 for f in sums} | {"buckets": {f: 0 for f in bucket_keys}, "cases": 0, "ok": 0}

    lines = [TRACE_COLUMNS]
    cats = {}
    total = empty()
    for c in per_case:
        lines.append(row_of(c["case"], c["category"], c["status"], c))
        fold(cats.setdefault(c["category"], empty()), c)
        fold(total, c)
    table = ["category\tcases\tran_ok\tpycg_edges\ttrace_edges\ttrace_and_pycg\tours_edges\tours_and_trace\trecall_of_covered_pct\tmatched\tcontradicted\tunjudged"]
    for cat, k in cats.items():
        lines.append(row_of(f"CATEGORY:{cat}", cat, f"{k['ok']}/{k['cases']}", k))
        table.append(
            f"{cat}\t{k['cases']}\t{k['ok']}\t{k['pycg']}\t{k['trace']}\t{k['trace_and_pycg']}\t{k['ours']}\t{k['ours_and_trace']}"
            f"\t{pct(k['ours_and_trace'], k['trace']):.2f}\t{k['buckets']['matched']}\t{k['buckets']['contradicted']}\t{k['buckets']['unjudged']}"
        )
    lines.append(row_of("TOTAL", "", f"{total['ok']}/{total['cases']}", total))
    table.append(
        f"TOTAL\t{total['cases']}\t{total['ok']}\t{total['pycg']}\t{total['trace']}\t{total['trace_and_pycg']}\t{total['ours']}\t{total['ours_and_trace']}"
        f"\t{pct(total['ours_and_trace'], total['trace']):.2f}\t{total['buckets']['matched']}\t{total['buckets']['contradicted']}\t{total['buckets']['unjudged']}"
    )
    trace_dir.mkdir(parents=True, exist_ok=True)
    (trace_dir / "SCORES.tsv").write_text("\n".join(lines) + "\n")
    with open(trace_dir / "MISSES.tsv", "w") as fh:
        for c in per_case:
            for m in c["misses"]:
                fh.write(f"{c['case']}\t{m}\n")
            for m in c["excess"]:
                fh.write(f"{c['case']}\tOURS_ONLY\t{m}\n")
    print("\n".join(table))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--suite", default=str(SUITE_DEFAULT))
    ap.add_argument("--extract", default=str(BENCH.parent / "v6/sprefa-extract/target/release/extract"))
    ap.add_argument("--out", default=str(HERE / "python-oracle"))
    ap.add_argument("--oracle", choices=("pycg", "trace"), default="pycg")
    ap.add_argument("--trace", default=None, help="trace dir holding TRACE.tsv and RUNS.tsv (default: <out>/trace)")
    args = ap.parse_args()
    suite = Path(args.suite)
    out_dir = Path(args.out)
    cases = sorted(p.parent for p in suite.glob("*/*/callgraph.json"))
    if args.oracle == "trace":
        score_trace(args, suite, out_dir, cases)
        return
    oracle_dir = out_dir / "oracle"
    ours_dir = out_dir / "ours"
    for d in (oracle_dir, ours_dir):
        d.mkdir(parents=True, exist_ok=True)

    print(f"{len(cases)} cases", file=sys.stderr)
    per_case = []
    for case_dir in cases:
        prefix = f"{case_dir.parent.name}/{case_dir.name}"
        name = prefix.replace("/", "__")
        oracle_rows = pycg_convert.convert(case_dir, prefix)
        (oracle_dir / f"{name}.call.tsv").write_text("\n".join(oracle_rows) + ("\n" if oracle_rows else ""))
        try:
            ours_rows = run_ours(args.extract, suite, case_dir, ours_dir, name)
        except subprocess.TimeoutExpired:
            print(f"TIMEOUT {name}", file=sys.stderr)
            ours_rows = []
        (ours_dir / f"{name}.call.tsv").write_text("\n".join(ours_rows) + ("\n" if ours_rows else ""))
        oracle_all = set(oracle_rows)
        external = {r for r in oracle_all if r.split("\t")[2] == f"{prefix}/{EXTERNAL}"}
        oracle = oracle_all - external
        ours = set(ours_rows)
        overlap = len(ours & oracle)
        rec = pct(overlap, len(oracle))
        prec = pct(overlap, len(ours))
        frec, fprec = fuzzy_score(ours, oracle) if (ours and oracle) else (0.0, 0.0)
        per_case.append(
            {
                "case": prefix,
                "category": case_dir.parent.name,
                "oracle": len(oracle),
                "external": len(external),
                "ours": len(ours),
                "overlap": overlap,
                "recall": rec,
                "precision": prec,
                "fuzzy_recall": frec,
                "fuzzy_precision": fprec,
                "misses": sorted(oracle - ours),
                "excess": sorted(ours - oracle),
            }
        )

    # SCORES.tsv: one row per case, then the per-category aggregate.
    lines = ["case\tcategory\toracle_edges\texternal_edges\tours_edges\toverlap\trecall_pct\tprecision_pct\tfuzzy_recall_pct\tfuzzy_precision_pct"]
    cats = {}
    for c in per_case:
        lines.append(
            f"{c['case']}\t{c['category']}\t{c['oracle']}\t{c['external']}\t{c['ours']}\t{c['overlap']}\t{c['recall']:.2f}\t{c['precision']:.2f}\t{c['fuzzy_recall']:.2f}\t{c['fuzzy_precision']:.2f}"
        )
        k = cats.setdefault(
            c["category"],
            {"cases": 0, "oracle": 0, "external": 0, "ours": 0, "overlap": 0},
        )
        k["cases"] += 1
        for f in ("oracle", "external", "ours", "overlap"):
            k[f] += c[f]
    for cat, k in cats.items():
        lines.append(
            f"CATEGORY:{cat}\t{cat}\t{k['cases']}\t{k['external']}\t{k['ours']}\t{k['overlap']}\t{pct(k['overlap'], k['oracle']):.2f}\t{pct(k['overlap'], k['ours']):.2f}\t\t"
        )
    (out_dir / "SCORES.tsv").write_text("\n".join(lines) + "\n")

    total_oracle = sum(c["oracle"] for c in per_case)
    total_external = sum(c["external"] for c in per_case)
    total_ours = sum(c["ours"] for c in per_case)
    total_overlap = sum(c["overlap"] for c in per_case)
    print(
        f"TOTAL oracle {total_oracle} external {total_external} ours {total_ours} "
        f"overlap {total_overlap} recall {pct(total_overlap, total_oracle):.2f} "
        f"precision {pct(total_overlap, total_ours):.2f}",
        file=sys.stderr,
    )
    # Dump misses for the report.
    with open(out_dir / "MISSES.tsv", "w") as fh:
        for c in per_case:
            for m in c["misses"]:
                fh.write(f"{c['case']}\t{m}\n")
            for m in c["excess"]:
                fh.write(f"{c['case']}\tOURS_ONLY\t{m}\n")


if __name__ == "__main__":
    main()
