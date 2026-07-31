#!/usr/bin/env python3
"""census.py -- the rules-deleted table for the point-free corpus.

  python3 v6/prolog/labs/point_free/census.py

Counts, per corpus program:
  rules today   -- rule statements in today/<name>.dl6
  decls today   -- `rel` declarations in today/<name>.dl6
  rules moved   -- rule statements the author writes in sugar/<name>.sugar.pl
  decls moved   -- `rel` declarations the sugar file still needs
  moves         -- which of M1/M2/M3 the sugar file uses

A program with no sugar file is counted unchanged: the moves buy it nothing,
and saying so is the point of including it.

Counting rule, stated because it decides the numbers: a RULE is one statement
containing `<-` or `<+` at top level (a `match` block counts as one rule per
arm, and this corpus has none). A DECL is one line beginning `rel `. Comment
lines and blank lines are ignored. The sugar side counts the same way over the
prolog term: one element of the Rules list is one rule, one `col_type` group
per ref is one decl.
"""

import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def strip_comments(text):
    return "\n".join(line for line in text.splitlines()
                     if not line.lstrip().startswith("#"))


def count_dl6(path):
    body = strip_comments(path.read_text())
    rules = len(re.findall(r"<-|<\+", body))
    decls = len(re.findall(r"^rel\s", body, re.MULTILINE))
    return rules, decls


def count_sugar(path):
    text = path.read_text()
    body = "\n".join(line for line in text.splitlines()
                     if not line.lstrip().startswith("%"))
    rules = len(re.findall(r"<-|<\+", body))
    refs = set(re.findall(r"col_type\((\w+/\d+)", body))
    return rules, len(refs)


def moves_used(path):
    # Comment-stripped: every sugar file quotes its rx equivalent in the header,
    # and rx spells `scan(` too, so reading the whole file credited the buffered
    # batch with an M1 it does not use.
    body = "\n".join(line for line in path.read_text().splitlines()
                     if not line.lstrip().startswith("%"))
    used = []
    if re.search(r"\bscan\(", body):
        used.append("M1")
    if re.search(r":=\s*seq\(", body):
        used.append("M2")
    if "~>" in body:
        used.append("M3")
    return "+".join(used) or "-"


def main():
    rows = []
    for today in sorted((HERE / "today").glob("*.dl6")):
        name = today.stem
        sugar = HERE / "sugar" / (name + ".sugar.pl")
        rules_today, decls_today = count_dl6(today)
        if sugar.exists():
            rules_moved, decls_moved = count_sugar(sugar)
            moves = moves_used(sugar)
        else:
            rules_moved, decls_moved, moves = rules_today, decls_today, "-"
        rows.append((name, moves, rules_today, rules_moved, decls_today, decls_moved))

    width = max(len(row[0]) for row in rows)
    header = f"{'program':<{width}}  moves   rules_today  rules_moved  deleted%  decls_today  decls_moved"
    print(header)
    print("-" * len(header))
    total_today = total_moved = 0
    total_decls_today = total_decls_moved = 0
    for name, moves, rules_today, rules_moved, decls_today, decls_moved in rows:
        deleted = 0 if rules_today == 0 else round(
            100 * (rules_today - rules_moved) / rules_today)
        print(f"{name:<{width}}  {moves:<6}  {rules_today:^11}  {rules_moved:^11}  "
              f"{deleted:^8}  {decls_today:^11}  {decls_moved:^11}")
        total_today += rules_today
        total_moved += rules_moved
        total_decls_today += decls_today
        total_decls_moved += decls_moved
    print("-" * len(header))
    deleted = round(100 * (total_today - total_moved) / total_today)
    print(f"{'TOTAL':<{width}}  {'':<6}  {total_today:^11}  {total_moved:^11}  "
          f"{deleted:^8}  {total_decls_today:^11}  {total_decls_moved:^11}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
