#!/usr/bin/env python3
"""3_classify.py -- bucket every v5/v6 difference in the multirepo crawl golden.

    3_classify.py <work-dir> <corpus-dir>   ->  exit 0 iff 0 unclassified

Same contract as flagship-classify.py: a non-empty diff with every row placed
in a bucket is an HONEST result and exits 0; a single row that cannot be placed
exits 1. The buckets:

  (a) extraction-input difference   the two engines read different facts out of
                                    the corpus. PROVEN per row against the
                                    go.mod bytes, not asserted.
  (b) v6 expression gap             the port could not say what v5 says.
  (c) real defect                   neither of the above.

THE DERIVED RELS ARE PROVEN DIFFERENTLY, AND MORE STRONGLY, than the extracted
one. `dep_pin` is extraction, so a differing row is checked against the corpus
bytes directly. `skewed` / `skew_row` / `skew_width` are DERIVED, so this file
re-implements v5's rule bodies ONCE and runs them against EACH engine's own
dep_pin set. Each engine must reproduce its own answer from its own inputs.
Both holding means the two engines compute the same function, so every
remaining difference is a function of the inputs and belongs to bucket (a).

That distinction is not decoration. In the flagship rig the equivalent
rule-fidelity leg is what caught a sabotage that row-shape proofs alone passed:
a changed rule body explains any row you like if all you ask is "do v5's inputs
derive it".
"""
import collections
import pathlib
import sys


def read_tsv(path):
    text = pathlib.Path(path).read_text()
    return {line for line in text.split("\n") if line}


def corpus_pins(corpus):
    """(repo, module, version) read straight out of the corpus go.mod bytes.

    Deliberately NOT either engine's regex: this is the third opinion the two
    legs are checked against, so a shared regex mistake cannot vindicate itself.
    A require line here is `<module-with-slash> <space> v<digit>...` and the
    corpus pins that shape (1_corpus.sh explains why the separator is a space).
    """
    pins = set()
    for gomod in sorted(pathlib.Path(corpus).glob("*/go.mod")):
        repo = gomod.parent.name
        for line in gomod.read_text().split("\n"):
            parts = line.split()
            if len(parts) != 2:
                continue
            module, version = parts
            if "/" in module and version.startswith("v") and version[1:2].isdigit():
                pins.add((repo, module, version))
    return pins


def v5_rules(pins):
    """v5's version-skew.dl rule bodies, written once, run on any dep_pin set.

    dep_ver(m, min(v), max(v)) <- dep_pin(_, m, v).
    skew(m, lo, hi)            <- dep_ver(m, lo, hi), lo != hi.
    skew_row(m, r, v)          <- skew(m,_,_), dep_pin(r, m, v).
    skew_width(m, count(r))    <- skew(m,_,_), dep_pin(r, m, _).

    The skew TEST is "more than one distinct version", which is what lo != hi
    means and is order-independent -- v5's own header says to treat lo/hi as
    two witnesses rather than oldest/newest, so this does not depend on which
    way round v5's min/max actually came out.
    """
    versions = collections.defaultdict(set)
    for _repo, module, version in pins:
        versions[module].add(version)
    skewed = {module for module, seen in versions.items() if len(seen) > 1}
    skew_row = {(m, r, v) for (r, m, v) in pins if m in skewed}
    width = collections.Counter(m for (m, _r, _v) in skew_row)
    return skewed, skew_row, dict(width)


def report(title, rows, limit=25):
    print(f"    {title}: {len(rows)}")
    for row in sorted(rows)[:limit]:
        print(f"      {row}")
    if len(rows) > limit:
        print(f"      ... and {len(rows) - limit} more")


def main():
    if len(sys.argv) != 3:
        sys.stderr.write("usage: 3_classify.py <work-dir> <corpus-dir>\n")
        return 2
    work, corpus = sys.argv[1], sys.argv[2]

    pins = corpus_pins(corpus)
    unclassified = 0

    print("CLASSIFY multirepo crawl")
    print(f"  corpus oracle: {len(pins)} (repo, module, version) pins read from go.mod bytes")

    # ── dep_pin: extraction. Prove every differing row against corpus bytes ──
    v5_pin = read_tsv(f"{work}/v5.dep_pin.tsv")
    v6_pin = read_tsv(f"{work}/v6.dep_pin.tsv")
    ground = {"\t".join(row) for row in pins}

    for label, seen, other in (("v5", v5_pin, v6_pin), ("v6", v6_pin, v5_pin)):
        only = seen - other
        if not only:
            continue
        print(f"  dep_pin {label}-only rows: {len(only)}")
        explained, unexplained = set(), set()
        for row in only:
            # (a) the row IS in the corpus and the other engine missed it, or
            #     the row is NOT in the corpus and this engine invented it.
            #     Either way the corpus bytes decide, and both are extraction
            #     differences only if the corpus agrees with exactly one side.
            (explained if row in ground else unexplained).add(row)
        report("(a) extraction-input, present in corpus bytes", explained)
        if unexplained:
            report("(c) UNCLASSIFIED, not derivable from corpus bytes", unexplained)
            unclassified += len(unexplained)

    if v5_pin == v6_pin:
        print(f"  dep_pin: both engines agree, {len(v5_pin)} rows")
        if v5_pin != ground:
            report("(c) UNCLASSIFIED, both engines disagree with the corpus bytes",
                   (v5_pin ^ ground))
            unclassified += len(v5_pin ^ ground)
        else:
            print("        and both match the corpus oracle exactly")

    # ── derived rels: rule fidelity, each engine against its OWN inputs ─────
    def pin_tuples(rows):
        return {tuple(row.split("\t")) for row in rows}

    print("  rule fidelity (v5's rule bodies run against each engine's own dep_pin):")
    for label, pin_rows, skewed_file, row_file, width_file in (
        ("v5", v5_pin, "v5.skewed.tsv", "v5.skew_row.tsv", "v5.skew_width.tsv"),
        ("v6", v6_pin, "v6.skewed.tsv", "v6.skew_row.tsv", "v6.skew_width.tsv"),
    ):
        skewed, skew_row, width = v5_rules(pin_tuples(pin_rows))
        got_skewed = read_tsv(f"{work}/{skewed_file}")
        got_row = read_tsv(f"{work}/{row_file}")
        got_width = read_tsv(f"{work}/{width_file}")

        want_skewed = set(skewed)
        want_row = {"\t".join(r) for r in skew_row}
        want_width = {f"{m}\t{n}" for m, n in width.items()}

        for name, want, got in (
            ("skewed", want_skewed, got_skewed),
            ("skew_row", want_row, got_row),
            ("skew_width", want_width, got_width),
        ):
            if want == got:
                print(f"    {label} {name}: FIDELITY OK ({len(got)} rows reproduced from its own dep_pin)")
            else:
                print(f"    {label} {name}: FIDELITY BROKEN")
                report("(c) UNCLASSIFIED, engine's own inputs do not derive its own answer",
                       (want ^ got))
                unclassified += len(want ^ got)

    # ── the named gap, restated so the exit line accounts for it ────────────
    print("  named gap (b) v6 expression gap:")
    print("    dep_ver / skew lo-hi columns: aggregate_operand_not_number(min, _, text)")
    print("    v6 refuses min/max over text; skew MEMBERSHIP is expressed as a")
    print("    self-join instead and grades in full, so the gap costs the two")
    print("    witness COLUMNS and no rows.")

    print(f"CLASSIFY unclassified={unclassified}")
    return 1 if unclassified else 0


if __name__ == "__main__":
    sys.exit(main())
