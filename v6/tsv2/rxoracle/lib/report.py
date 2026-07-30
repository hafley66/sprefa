#!/usr/bin/env python3
"""report.py -- compare one case's two normalized line files and rule on it.

Both inputs are already in the shared format and already sorted (README
sections 1 and 3; `sort` over a zero-padded step field IS normalization N2).
This script only compares and classifies; it applies no transformation of its
own, so no diff can be made to pass from here.

Verdicts, printed as the last line of stdout as `VERDICT <word>`:

  exact     the two files are byte-identical and the case declared no opt-in
            normalization
  modulo    the two files are identical and the case opted into N3 and/or N4
            exceptions, which are named in the output
  diverges  the files differ

Exit code 0 when the verdict equals the case's declared expectation, 1 when it
does not. An expected divergence that quietly starts matching fails just as
loudly as a match that breaks: both mean the recorded finding is stale.
"""

import argparse
import sys


def read(path: str) -> list[str]:
    with open(path, encoding="utf-8") as handle:
        return [line.rstrip("\n") for line in handle if line.strip()]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--case", required=True)
    parser.add_argument("--leg-a", required=True)
    parser.add_argument("--leg-b", required=True)
    parser.add_argument("--expect", required=True, choices=["exact", "modulo", "diverges"])
    parser.add_argument("--applied", default="", help="comma separated opt-in normalizations")
    arguments = parser.parse_args()

    left = read(arguments.leg_a)
    right = read(arguments.leg_b)
    applied = [name for name in arguments.applied.split(",") if name]

    only_left = [line for line in left if line not in right]
    only_right = [line for line in right if line not in left]
    # Multiplicity matters (a case whose whole point is "N events became 1"),
    # so compare as ordered lists, not sets.
    identical = left == right

    if identical:
        verdict = "modulo" if applied else "exact"
    else:
        verdict = "diverges"

    print(f"case      {arguments.case}")
    print(f"expect    {arguments.expect}")
    print(f"leg A     {len(left)} lines")
    print(f"leg B     {len(right)} lines")
    if applied:
        print(f"opt-in    {' '.join(applied)}")
    if not identical:
        print("")
        print("  rxjs only (leg A has it, sprefa does not):")
        for line in only_left or ["    (none)"]:
            print(f"    {line}" if line.strip() != "(none)" else line)
        print("  sprefa only (leg B has it, rxjs does not):")
        for line in only_right or ["    (none)"]:
            print(f"    {line}" if line.strip() != "(none)" else line)
        if not only_left and not only_right:
            print("  (same multiset, different multiplicity or step ordering)")
            print("  leg A:")
            for line in left:
                print(f"    {line}")
            print("  leg B:")
            for line in right:
                print(f"    {line}")
    print(f"VERDICT   {verdict}")

    if verdict != arguments.expect:
        sys.stderr.write(
            f"FAIL {arguments.case}: expected {arguments.expect}, measured {verdict}\n"
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
