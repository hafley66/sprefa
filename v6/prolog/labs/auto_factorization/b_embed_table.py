"""Splice the generated rename table into the verdict so the verdict stands
alone once the lab is deleted. Idempotent: re-running replaces the block.
"""
import re
from pathlib import Path

LAB = Path(__file__).resolve().parent
REPO = LAB.parents[3]
VERDICT = REPO / "plans/2026-07-31-auto-factorization-verdict.md"
MARKER = "<!-- RENAME-TABLE -->"
HEADER = "| package | current path |"

table = (LAB / "out/rename_table.md").read_text().strip()
text = VERDICT.read_text()

if MARKER in text:
    head, _sep, tail = text.partition(MARKER)
    body = "\n### 6g." + tail.split("\n### 6g.", 1)[1]
else:
    start = text.index(HEADER)
    head = text[:start]
    body = "\n### 6g." + text[start:].split("\n### 6g.", 1)[1]

VERDICT.write_text(head + table + "\n" + body)
print(f"spliced {len(table.splitlines()) - 2} rows")
