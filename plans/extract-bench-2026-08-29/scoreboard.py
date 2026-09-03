#!/usr/bin/env python3
"""Print the sprefa-extract scoreboard as markdown tables, every cell read
from a ledger in the repo (never typed by hand):

  RATCHET.tsv, RATCHET.cost.tsv            tuning-corpus floors and cost
  ../extract-eval-2026-08-31/heldout/SCORES.tsv   held-out repos vs SCIP
  python-oracle/SCORES.tsv, python-oracle/trace/SCORES.tsv   PyCG suite
  OPEN-PROBLEMS.md                          the open-problem index
  git ls-files                              size counts

Usage: scoreboard.py [--out FILE]   (default: stdout)
"""
from __future__ import annotations

import argparse
import csv
import subprocess
from collections import defaultdict
from pathlib import Path

BENCH = Path(__file__).resolve().parent
REPO = BENCH.parent.parent
HELDOUT = REPO / "plans" / "extract-eval-2026-08-31" / "heldout" / "SCORES.tsv"
CRATE = REPO / "v6" / "sprefa-extract"


def read_tsv(path: Path) -> list[dict[str, str]]:
    with path.open() as handle:
        rows = [line for line in handle if not line.startswith("#")]
    return list(csv.DictReader(rows, delimiter="\t"))


def table(headers: list[str], rows: list[list[str]]) -> str:
    out = ["| " + " | ".join(headers) + " |", "|" + "---|" * len(headers)]
    out.extend("| " + " | ".join(row) + " |" for row in rows)
    return "\n".join(out)


def git_files(prefix: str) -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", prefix], cwd=REPO, capture_output=True, text=True, check=True
    )
    return [line for line in result.stdout.splitlines() if line]


def size_section() -> str:
    src = [path for path in git_files("v6/sprefa-extract/src") if path.endswith(".rs")]
    lines = sum(sum(1 for _ in (REPO / path).open(errors="replace")) for path in src)
    tests = [path for path in git_files("v6/sprefa-extract/tests") if path.endswith(".rs")]
    arms = [path for path in src if "/src/lang/" in path]
    sha = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"], cwd=REPO, capture_output=True, text=True
    ).stdout.strip()
    return table(
        ["measure", "value"],
        [
            ["tree", sha],
            ["src .rs lines", f"{lines:,}"],
            ["src .rs files", str(len(src))],
            ["test .rs files", str(len(tests))],
            ["language arm files", str(len(arms))],
        ],
    )


def ratchet_section() -> str:
    rows = read_tsv(BENCH / "RATCHET.tsv")
    cells: dict[tuple[str, str, str], dict[str, str]] = defaultdict(dict)
    for row in rows:
        key = (row["lang"], row["family"], row["oracle"])
        cells[key][row["tier"]] = f'{row["recall"]} / {row["precision"]}'
    out = []
    for (lang, family, oracle), tiers in sorted(cells.items()):
        out.append(
            [lang, family, oracle, tiers.get("syntax", ""), tiers.get("checker", "")]
        )
    return table(["lang", "family", "oracle", "syntax recall / precision", "checker recall / precision"], out)


def cost_section() -> str:
    rows = read_tsv(BENCH / "RATCHET.cost.tsv")
    return table(
        ["lang", "tier", "files", "wall ms", "rss MB", "measured at"],
        [[r["lang"], r["tier"], r["files"], r["wall_ms"], r["rss_mb"], r["measured_at_sha"]] for r in rows],
    )


def heldout_section() -> str:
    if not HELDOUT.exists():
        return "_no held-out ledger_"
    rows = read_tsv(HELDOUT)
    by_repo: dict[tuple[str, str, str], dict[str, dict[str, str]]] = defaultdict(dict)
    for row in rows:
        by_repo[(row["lang"], row["corpus_class"], row["repo"])][row["tier"]] = row
    out = []
    for (lang, corpus_class, repo), tiers in sorted(by_repo.items()):
        syntax = tiers.get("syntax", {})
        checker = tiers.get("checker", {})
        decline = checker.get("tier_decline", "") or syntax.get("tier_decline", "")
        decline = decline.split(":")[0] if decline else ""
        out.append(
            [
                lang,
                corpus_class,
                repo,
                syntax.get("files", checker.get("files", "")),
                syntax.get("recall", ""),
                checker.get("recall", ""),
                decline,
            ]
        )
    return table(["lang", "class", "repo", "files", "syntax recall", "checker recall", "tier decline"], out)


def pycg_section() -> str:
    rows = read_tsv(BENCH / "python-oracle" / "SCORES.tsv")
    cases = [r for r in rows if not r["case"].startswith("CATEGORY:")]
    categories = [r for r in rows if r["case"].startswith("CATEGORY:")]
    oracle = sum(int(r["oracle_edges"]) for r in cases)
    overlap = sum(int(r["overlap"]) for r in cases)
    ours = sum(int(r["ours_edges"]) for r in cases)
    trace_path = BENCH / "python-oracle" / "trace" / "SCORES.tsv"
    trace = {r["case"]: r for r in read_tsv(trace_path)} if trace_path.exists() else {}
    total = trace.get("TOTAL", {})
    head = table(
        ["measure", "value"],
        [
            ["cases", str(len(cases))],
            ["PyCG recall", f"{100 * overlap / oracle:.2f} = {overlap} / {oracle}"],
            ["PyCG precision", f"{100 * overlap / ours:.2f} = {overlap} / {ours}"],
            ["trace mains ran", total.get("status", "")],
            ["trace recall of covered", total.get("recall_of_covered_pct", "")],
        ],
    )
    body = table(
        ["category", "PyCG recall", "PyCG precision", "trace recall of covered"],
        [
            [
                r["category"],
                r["recall_pct"],
                r["precision_pct"],
                trace.get("CATEGORY:" + r["category"], {}).get("recall_of_covered_pct", ""),
            ]
            for r in categories
        ],
    )
    return head + "\n\n" + body


def open_problems_section() -> str:
    out = []
    for line in (BENCH / "OPEN-PROBLEMS.md").read_text().splitlines():
        if not line.startswith("| ") or line.startswith("| #") or line.startswith("|---"):
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if len(cells) < 3 or not cells[0].isdigit():
            continue
        numbers = cells[2]
        state = "CLOSED" if numbers.upper().startswith("CLOSED") or "CLOSED" in cells[1].upper() else "open"
        out.append([cells[0], cells[1], state, numbers[:90] + ("..." if len(numbers) > 90 else "")])
    out.sort(key=lambda row: int(row[0]))
    return table(["#", "problem", "state", "numbers"], out)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    sections = [
        ("Size", size_section()),
        ("Tuning corpora (RATCHET.tsv)", ratchet_section()),
        ("Cost (RATCHET.cost.tsv)", cost_section()),
        ("Held-out repos vs SCIP (heldout/SCORES.tsv)", heldout_section()),
        ("Python PyCG suite and trace oracle", pycg_section()),
        ("Open problems (OPEN-PROBLEMS.md)", open_problems_section()),
    ]
    doc = ["# sprefa-extract scoreboard", "", "Generated by `plans/extract-bench-2026-08-29/scoreboard.py`; every cell is read from a ledger. Regenerate: `just extract-scoreboard`.", ""]
    doc.append("## TOC")
    doc.extend(f"- {title}" for title, _ in sections)
    for title, body in sections:
        doc.extend(["", f"## {title}", "", body])
    text = "\n".join(doc) + "\n"
    if args.out:
        args.out.write_text(text)
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
