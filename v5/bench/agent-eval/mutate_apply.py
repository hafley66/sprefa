#!/usr/bin/env python3
"""mutate_apply.py <mutation_kind> <target_file> <site_line> <site_path>

The text-tool mutation engine mutate.sh shells out to. Stdlib only, no
dependency on dl or on gen-tasks.dl's own regexes beyond re-finding the
already-identified site line — this is the anti-grift protocol's ground-truth
authority (plans/2026-07-10-agent-eval-harness.md #1): the answer key it
prints is computed from the mutated text itself, never from dl's graph.

<site_line> is 1-based (dl's `match` line numbers are 1-based, matching every
editor/compiler convention). Prints "<answer_file>\t<answer_line>" (both
1-based) to stdout: the exact coordinates of the mutation just applied.
"""
import re
import sys


def read_lines(path):
    with open(path, "r", encoding="utf-8") as f:
        return f.readlines()


def write_lines(path, lines):
    with open(path, "w", encoding="utf-8") as f:
        f.writelines(lines)


def mutate_import_break(lines, site_line):
    """`use crate::a::b;` -> `use crate::a::b_typo;` (same line)."""
    idx = site_line - 1
    original = lines[idx]
    mutated = re.sub(r"([A-Za-z0-9_]+);(\s*)$", r"\1_typo;\2", original, count=1)
    if mutated == original:
        raise SystemExit(f"import-break: no import path to corrupt on line {site_line}: {original!r}")
    lines[idx] = mutated
    return site_line


def mutate_export_orphan(lines, site_line):
    """`pub fn name(` -> `fn name(` (same line; strips the export)."""
    idx = site_line - 1
    original = lines[idx]
    mutated = re.sub(r"^pub fn ", "fn ", original, count=1)
    if mutated == original:
        raise SystemExit(f"export-orphan: line {site_line} is not a top-level `pub fn`: {original!r}")
    lines[idx] = mutated
    return site_line


def find_fn_span(lines, site_line):
    """Brace-counted [start, end) line range (0-based, end exclusive) of the
    function starting at 1-based site_line."""
    start = site_line - 1
    depth = 0
    seen_open = False
    end = start
    for i in range(start, len(lines)):
        for ch in lines[i]:
            if ch == "{":
                depth += 1
                seen_open = True
            elif ch == "}":
                depth -= 1
        if seen_open and depth == 0:
            end = i + 1
            return start, end
    raise SystemExit(f"duplicate-sym: unbalanced braces from line {site_line}")


def mutate_duplicate_sym(lines, site_line):
    """Duplicate the whole function body right after itself: a second
    definition with the same name (E0428-shaped defect). Answer = the first
    line of the newly inserted (second) copy."""
    start, end = find_fn_span(lines, site_line)
    span = lines[start:end]
    # A blank separator line keeps the duplicate visually distinct without
    # changing brace/line-counting math.
    lines[end:end] = ["\n"] + span
    second_def_line = end + 1 + 1  # 1-based: end (0-based, first line of span) + separator + 1
    return second_def_line


def mutate_off_by_one(lines, site_line):
    """`for x in 0..N {` -> `for x in 0..N+1 {` (same line; a literal bound
    nudge, not the closure-form N+1 — keeps the line a single textual edit)."""
    idx = site_line - 1
    original = lines[idx]

    def bump(m):
        return f"0..{int(m.group(1)) + 1} {{"

    mutated = re.sub(r"0\.\.([0-9]+) \{", bump, original, count=1)
    if mutated == original:
        raise SystemExit(f"off-by-one: no `0..N {{` bound on line {site_line}: {original!r}")
    lines[idx] = mutated
    return site_line


MUTATORS = {
    "import-break": mutate_import_break,
    "export-orphan": mutate_export_orphan,
    "duplicate-sym": mutate_duplicate_sym,
    "off-by-one": mutate_off_by_one,
}


def main():
    if len(sys.argv) != 5:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    mutation_kind, target_file, site_line_str, site_path = sys.argv[1:5]
    site_line = int(site_line_str)
    mutator = MUTATORS.get(mutation_kind)
    if mutator is None:
        raise SystemExit(f"unknown mutation_kind {mutation_kind!r}; known: {sorted(MUTATORS)}")

    lines = read_lines(target_file)
    answer_line = mutator(lines, site_line)
    write_lines(target_file, lines)
    print(f"{site_path}\t{answer_line}")


if __name__ == "__main__":
    main()
