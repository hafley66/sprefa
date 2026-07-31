"""Splice the generated rename table into the verdict at its marker, so the
verdict stands alone once the lab is deleted.
"""
from pathlib import Path

LAB = Path(__file__).resolve().parent
REPO = LAB.parents[3]
VERDICT = REPO / "plans/2026-07-31-auto-factorization-verdict.md"
MARKER = "<!-- RENAME-TABLE -->"

table = (LAB / "out/rename_table.md").read_text().strip()
text = VERDICT.read_text()
head, _sep, tail = text.partition(MARKER)
if not _sep:
    raise SystemExit("marker absent; table already spliced or verdict edited")
after = tail.split("\n### 6g.", 1)
VERDICT.write_text(head + table + "\n\n### 6g." + after[1])
print(f"spliced {len(table.splitlines()) - 2} rows")
