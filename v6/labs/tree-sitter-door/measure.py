#!/usr/bin/env python3
"""Overlay ratio for the tree-sitter door.

Rule spans start at a four-space rule key and end at the next rule key, the
same unit round 1 and round 2 counted.  A rule counts as generated only when
the emitter wrote its body and that body matches grammar.js character for
character; classification.tsv carries round 2's weaker shape verdicts for the
rules the emitter still does not write.
"""
import re
import sys
from pathlib import Path

LAB = Path(__file__).resolve().parent
RULE_KEY = re.compile(r"^    ([a-z_][a-z0-9_]*): \$ =>")
RULES_END = re.compile(r"^  \},")
GENERATED_HEADER = re.compile(r"^// Generated rule bodies: \[(.*)\]$", re.M)


def rule_bodies(text):
    lines = text.splitlines(keepends=True)
    starts = [i for i, line in enumerate(lines) if RULE_KEY.match(line)]
    last = next(i for i, line in enumerate(lines)
                if RULES_END.match(line) and i > starts[0])
    bodies = {}
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else last
        name = RULE_KEY.match(lines[start]).group(1)
        bodies[name] = "".join(lines[start:end])
    return bodies


def squeeze(text):
    return re.sub(r"\s", "", text)


def main():
    hand = (LAB / "grammar.js").read_text()
    emitted = (LAB / "emitted-grammar.js").read_text()
    verdicts = {}
    for line in (LAB / "classification.tsv").read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        name, verdict, reason = line.split("\t", 2)
        verdicts[name] = (verdict, reason)

    hand_bodies = rule_bodies(hand)
    emitted_bodies = rule_bodies(emitted)
    hand_sizes = {name: len(squeeze(body)) for name, body in hand_bodies.items()}

    missing = sorted(set(hand_bodies) - set(verdicts))
    stale = sorted(set(verdicts) - set(hand_bodies))
    if missing or stale:
        print(f"CLASSIFICATION DRIFT missing={missing} stale={stale}",
              file=sys.stderr)
        return 1

    claimed = GENERATED_HEADER.search(emitted).group(1)
    claimed = [name.strip() for name in claimed.split(",") if name.strip()]
    generated = []
    for name in claimed:
        if squeeze(emitted_bodies[name]) != squeeze(hand_bodies[name]):
            print(f"GENERATED BODY DIVERGES rule={name}", file=sys.stderr)
            print(f"  emitted {squeeze(emitted_bodies[name])}", file=sys.stderr)
            print(f"  hand    {squeeze(hand_bodies[name])}", file=sys.stderr)
            return 1
        generated.append(name)

    helpers = sum(len(squeeze(body)) for name, body in emitted_bodies.items()
                  if name not in hand_bodies)

    def verdict_of(name):
        return "EMITTED-IDENTICAL" if name in generated else verdicts[name][0]

    def basis_of(name):
        if name in generated:
            return "generated"
        if verdicts[name][0] == "EMITTED-IDENTICAL":
            return "shape"
        return "hand"

    identical = sum(size for name, size in hand_sizes.items()
                    if verdict_of(name) == "EMITTED-IDENTICAL")
    overlay = sum(size for name, size in hand_sizes.items()
                  if verdict_of(name) != "EMITTED-IDENTICAL")
    strict_identical = sum(size for name, size in hand_sizes.items()
                           if name in generated)
    strict_overlay = sum(size for name, size in hand_sizes.items()
                         if name not in generated)
    emitted_total = identical + helpers
    strict_total = strict_identical + helpers

    counts = {}
    for name in hand_sizes:
        key = verdict_of(name)
        counts[key] = counts.get(key, 0) + 1
    bases = {}
    for name in hand_sizes:
        key = basis_of(name)
        bases[key] = bases.get(key, 0) + 1

    print(f"identical hand-rule spans          {identical:6d}")
    print(f"generated specialized helper rules {helpers:6d}")
    print(f"emitted total                      {emitted_total:6d}")
    print(f"remaining hand-rule overlay        {overlay:6d}")
    print(f"ratio                              {overlay / emitted_total:.4f}")
    print(f"strict emitted total               {strict_total:6d}")
    print(f"strict overlay                     {strict_overlay:6d}")
    print(f"strict ratio                       "
          f"{strict_overlay / strict_total:.4f}")
    print("rules " + " ".join(f"{k}={v}" for k, v in sorted(counts.items())))
    print("basis " + " ".join(f"{k}={v}" for k, v in sorted(bases.items())))
    print("generated " + " ".join(generated))
    return 0


if __name__ == "__main__":
    sys.exit(main())
