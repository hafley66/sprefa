#!/usr/bin/env python3
"""csp_census.py -- re-measure the M2 claim against a corpus this lab did not
write.

  python3 v6/prolog/labs/point_free/csp_census.py

The csp-idioms verdict states "73 of 94 rules are verbatim-shape repeats, and
the single template cursor/pending/item/ready accounts for 52 of them". That
lab is nine cold-authored CSP programs, which makes it the only independent
evidence for M2 -- so the number M2 is priced on is re-derived here from the
files rather than quoted.

What is counted: a rule is one statement (statements are separated by `.` at
end of line; continuation lines are joined first) containing `<-` or `<+`
outside a comment. A CURSOR-TEMPLATE rule is one whose body reads a cursor rel
through `not(...)` or `pre(...)` -- those are exactly the four rules `seq`
replaces, in the two-plus-two shape card 1a spells out.

M2's saving per cursor block is 4 rules -> 1: the two cursor-maintenance rules
disappear entirely and the two payload rules (base case and step case) collapse
into the single rule that carries the `:= seq(...)` bind.
"""

import re
import sys
from pathlib import Path

CSP = Path(__file__).resolve().parent.parent / "csp_idioms"
GRADED = ["buffered", "workerpool", "pipeline", "fanin", "fanout",
          "select", "timeout", "done", "rendezvous", "semaphore"]


def statements(path):
    lines = []
    for line in path.read_text().splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        lines.append(stripped)
    joined = " ".join(lines)
    return [s.strip() for s in joined.split(".") if s.strip()]


def main():
    total_rules = 0
    total_template = 0
    total_blocks = 0
    print(f"{'idiom':<12} {'rules':>6} {'cursor-template':>16} {'cursor rels':>12} {'rules after M2':>15}")
    print("-" * 66)
    for name in GRADED:
        path = CSP / (name + ".dl6")
        if not path.exists():
            print(f"{name:<12} MISSING")
            continue
        rules = [s for s in statements(path) if "<-" in s or "<+" in s]
        template = [s for s in rules
                    if re.search(r"not\(\s*\w*cursor\w*\(", s)
                    or re.search(r"pre\(\s*\w*cursor\w*\(", s)]
        cursor_rels = set(re.findall(r"(?:not|pre)\(\s*(\w*cursor\w*)\(", " ".join(rules)))
        blocks = len(cursor_rels)
        after = len(rules) - len(template) + blocks
        total_rules += len(rules)
        total_template += len(template)
        total_blocks += blocks
        print(f"{name:<12} {len(rules):>6} {len(template):>16} {blocks:>12} {after:>15}")
    print("-" * 66)
    after = total_rules - total_template + total_blocks
    deleted = round(100 * (total_rules - after) / total_rules)
    print(f"{'TOTAL':<12} {total_rules:>6} {total_template:>16} {total_blocks:>12} {after:>15}"
          f"   ({deleted}% deleted)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
