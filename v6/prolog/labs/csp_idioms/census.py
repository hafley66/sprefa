#!/usr/bin/env python3
"""census.py : the boilerplate census, counted mechanically rather than by eye.

Every rule in every lab program is normalised to its SHAPE -- rel names, atom
constants, variables and integers are all replaced by placeholders, so only the
structure (which goals, in which order, with which operators) survives. Two
rules with the same normalised text are verbatim-shape repeats of each other.

Counting this way rather than by hand is deliberate: the ruling that asked for
this census (stream card 1b, `seq(name)` sugar) should be confirmed or amended
against a count nobody tuned.

    python3 census.py
"""
import glob
import os
import re
import sys
from collections import Counter, defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))

# programs in idiom order; the naive semaphore is excluded from the census
# because it is a receipt for an error, not an idiom implementation.
ORDER = [
    ("1  buffered channel", "buffered.dl6"),
    ("2  worker pool", "workerpool.dl6"),
    ("3  pipeline", "pipeline.dl6"),
    ("4a fan-in", "fanin.dl6"),
    ("4b fan-out", "fanout.dl6"),
    ("5  select", "select.dl6"),
    ("6  timeout", "timeout.dl6"),
    ("7  done channel", "done.dl6"),
    ("8  rendezvous", "rendezvous.dl6"),
    ("9  semaphore", "semaphore.dl6"),
]

TOKEN = re.compile(r"'[^']*'|[A-Z_][A-Za-z0-9_]*|[a-z][A-Za-z0-9_]*|\d+|\S")


def statements(text):
    """Split a .dl6 into statements, dropping comments and blank lines.

    The rstrip('.') matters: a file whose last line carries no trailing newline
    keeps its own period through the split, and without this the final rule of
    every program normalises to a shape ending '. .' that matches nothing. That
    bug undercounted one rule per file on the first run of this census.
    """
    body = "\n".join(l for l in text.splitlines() if not l.strip().startswith("#"))
    return [s.strip().rstrip(".") + "." for s in body.split(".\n") if s.strip()]


def normalise(stmt):
    """Replace every name/constant/variable/integer with a placeholder."""
    out = []
    for tok in TOKEN.findall(stmt):
        if tok.startswith("'"):
            out.append("'c'")
        elif tok[0].isupper() or tok[0] == "_":
            out.append("V")
        elif tok[0].islower():
            out.append("n")
        elif tok.isdigit():
            out.append("0")
        else:
            out.append(tok)
    return " ".join(out)


def main():
    shapes = defaultdict(list)          # normalised shape -> [(idiom, stmt)]
    rows = []
    for label, filename in ORDER:
        path = os.path.join(HERE, filename)
        if not os.path.exists(path):
            print("missing: %s" % filename, file=sys.stderr)
            continue
        with open(path) as handle:
            stmts = statements(handle.read())
        decls = [s for s in stmts if s.startswith("rel ")]
        rules = [s for s in stmts if not s.startswith("rel ")]
        for rule in rules:
            shapes[normalise(rule)].append((label, rule))
        rows.append((label, len(decls), len(rules), rules))

    # a shape is REPEATED if it occurs more than once anywhere in the corpus
    repeated = {shape for shape, uses in shapes.items() if len(uses) > 1}

    print("== boilerplate census ==")
    print("%-22s %6s %6s %9s %7s" % ("idiom", "decls", "rules", "repeated", "novel"))
    total_rules = total_rep = 0
    for label, ndecls, nrules, rules in rows:
        rep = sum(1 for r in rules if normalise(r) in repeated)
        total_rules += nrules
        total_rep += rep
        print("%-22s %6d %6d %9d %7d" % (label, ndecls, nrules, rep, nrules - rep))
    print("%-22s %6s %6d %9d %7d" % ("TOTAL", "", total_rules, total_rep,
                                     total_rules - total_rep))

    print()
    print("== the repeated shapes, most-repeated first ==")
    ranked = sorted(((len(u), s, u) for s, u in shapes.items() if len(u) > 1),
                    reverse=True)
    for count, shape, uses in ranked:
        idioms = Counter(label for label, _ in uses)
        print("\n%d occurrences, in %d idioms (%s)"
              % (count, len(idioms), ", ".join(sorted(i.split()[0] for i in idioms))))
        print("  example: %s" % uses[0][1].replace("\n", " "))


if __name__ == "__main__":
    main()
