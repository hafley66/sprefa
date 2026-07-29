"""flagship-classify.py — the honesty half of the flagship rig.

flagship-callgraph.sh produces eight sorted TSVs (v5 and v6, four rels). This
file turns every row of difference into a BUCKET, and it PROVES the bucket from
the corpus bytes rather than asserting it:

  (a) extraction-input difference   the two extractors saw different facts
  (b) v6 expression gap             the port could not say what v5 says
  (c) real defect                   neither of the above

The four proofs, one per rel:

  def     a v6-only def must be a BODY-LESS declaration. `sprefa-extract` gives
          the node's byte span; a Rust `fn` whose span carries no `{` is a trait
          method signature, which tree-sitter calls `function_signature_item`
          and v5's `(function_item name: (identifier) @name)` cannot match.
          Checked against the file bytes.

  call    a v6-only (path, callee) must have NO BARE-IDENTIFIER occurrence in
          that file. v5's `(call_expression function: (identifier) @callee)`
          matches a call whose callee is a plain `identifier` node and nothing
          else, so three shapes are invisible to it and each is proven from the
          extractor's own output plus the file bytes:
            path call     `callee_path` is non-null (`Vec::new`, `Cli::parse`);
                          the callee is a `scoped_identifier`, and the site span
                          covers the WHOLE path, which is why a byte-prefix test
                          alone gets this wrong.
            method call   the byte before the span is `.` (`x.iter()`); the
                          callee is a `field_expression`.
            struct literal the next non-space byte after the span is `{`
                          (`ExtractOutput { .. }`); syn lifts a struct
                          expression as a constructor call, tree-sitter's
                          `call_expression` never matches one.
          One bare occurrence anywhere in the file would have put the row in
          BOTH sets, so "every occurrence" is the right quantifier.

  calls   and

  unused  are DERIVED, so their proof is a RULE-FIDELITY CHECK on both sides
          rather than a story about rows. `derive_calls` / `derive_unused` below
          are v5's two rule bodies written once in Python, and the classifier
          runs them against EACH engine's own def/call sets:

              derive(v5.def, v5.call) == v5.calls      (v5's engine)
              derive(v6.def, v6.call) == v6.calls      (v6's engine)

          Both holding means the two engines compute THE SAME FUNCTION and every
          remaining row of difference is a function of the inputs, which is
          bucket (a) by construction. Either failing is bucket (c) and is
          reported per row. This is what catches a port that quietly drops a
          body atom: restricting v6's answer to v5's inputs would still "explain"
          the extra rows (measured — the earlier draft of this file passed that
          sabotage), while a rule-fidelity check fails it on the first row.

          `unused` additionally has the property worth naming: it is ANTI-monotone
          in `call`, so where v6's call set is a superset its `unused` set is a
          SUBSET. The table's v5only/v6only columns invert for that one rel and
          that is the semantics working, not a defect.

Exit 0 when every difference is classified, 1 when any row is left over. An
unexplained row is a failure of the rig, never a footnote.
"""

import json
import os
import subprocess
import sys


def read_tsv(path):
    if not os.path.exists(path):
        return set()
    with open(path, encoding="utf-8") as handle:
        return {tuple(line.rstrip("\n").split("\t")) for line in handle if line.strip()}


def extractor_facts(root, extract_bin, files):
    """Re-read the extractor once per file: (nodes, sites) keyed by path.

    The rig already ran these rows through the served engine; this second read
    is what lets the classifier see the SPANS, which the program cannot carry
    (the nested-span gap named in the .dl6 header)."""
    nodes, sites = {}, {}
    for path in files:
        out = subprocess.run(
            [extract_bin, "--family", "call", path],
            cwd=root, capture_output=True, text=True, check=False,
        ).stdout
        for line in out.splitlines():
            fact = json.loads(line)
            if fact["record"] == "node" and fact.get("name") is not None:
                nodes.setdefault(path, []).append(fact)
            elif fact["record"] == "site":
                sites.setdefault(path, []).append(fact)
    return nodes, sites


def file_bytes(root, path):
    with open(os.path.join(root, path), "rb") as handle:
        return handle.read()


def classify_def(root, only_v6, nodes):
    """A v6-only def is (a) when its span carries no body."""
    explained, left = [], []
    for path, name in sorted(only_v6):
        spans = [n["span"] for n in nodes.get(path, []) if n["name"] == name]
        if spans and all(b"{" not in file_bytes(root, path)[s["start"]:s["end"]] for s in spans):
            explained.append((path, name, "body-less declaration (trait method signature)"))
        else:
            left.append((path, name))
    return explained, left


def occurrence_shape(data, site):
    """Why v5's `(call_expression function: (identifier))` cannot see this site,
    or None when it could have."""
    if site.get("callee_path"):
        return "path call"
    start, end = site["span"]["start"], site["span"]["end"]
    if data[max(0, start - 1):start] == b".":
        return "method call"
    tail = data[end:end + 64].lstrip()
    if tail.startswith(b"{"):
        return "struct literal"
    return None


def classify_call(root, only_v6, sites):
    """A v6-only call is (a) when NO occurrence is a bare-identifier call."""
    explained, left = [], []
    for path, callee in sorted(only_v6):
        data = file_bytes(root, path)
        occurrences = [s for s in sites.get(path, []) if s["callee"] == callee]
        shapes = [occurrence_shape(data, s) for s in occurrences]
        if occurrences and all(shape is not None for shape in shapes):
            explained.append((path, callee, " + ".join(sorted(set(shapes)))))
        else:
            left.append((path, callee))
    return explained, left


def derive_calls(defs, calls):
    """v5's rule, once:
       calls(caller, callee) <- def(caller, p, _), call(callee, p, _),
                                def(callee, _, _), caller != callee."""
    names = {name for _, name in defs}
    by_path_def, by_path_call = {}, {}
    for path, name in defs:
        by_path_def.setdefault(path, set()).add(name)
    for path, callee in calls:
        by_path_call.setdefault(path, set()).add(callee)
    return {
        (caller, callee)
        for path, callers in by_path_def.items()
        for caller in callers
        for callee in by_path_call.get(path, ())
        if callee in names and caller != callee
    }


def derive_unused(defs, calls):
    """v5's rule, once: unused(name) <- def(name, _, _), !call(name, _, _)."""
    called = {callee for _, callee in calls}
    return {(name,) for _, name in defs if name not in called}


def rule_fidelity(rel, derive, sets):
    """Run v5's own rule against EACH engine's inputs and demand its own answer
    back. A mismatch is bucket (c) on that engine, per row."""
    findings, left = [], []
    for index, engine in ((0, "v5"), (1, "v6")):
        expected = derive(sets["def"][index], sets["call"][index])
        actual = sets[rel][index]
        if expected == actual:
            findings.append(f"{engine} engine reproduces v5's {rel} rule exactly ({len(actual)} rows)")
        else:
            for row in sorted(expected - actual):
                left.append((f"{engine}-missing",) + row)
            for row in sorted(actual - expected):
                left.append((f"{engine}-extra",) + row)
    return findings, left


def main():
    work, root, extract_bin = sys.argv[1], sys.argv[2], sys.argv[3]
    files = sorted(
        line.strip()
        for line in open(os.path.join(work, "fileset.pin"), encoding="utf-8")
        if line.strip()
    )
    nodes, sites = extractor_facts(root, extract_bin, files)

    sets = {}
    for rel in ("def", "call", "calls", "unused"):
        sets[rel] = (
            read_tsv(os.path.join(work, f"v5.{rel}.tsv")),
            read_tsv(os.path.join(work, f"v6.{rel}.tsv")),
        )

    rows, leftovers = [], []

    for rel in ("def", "call", "calls", "unused"):
        v5, v6 = sets[rel]
        both, only5, only6 = v5 & v6, v5 - v6, v6 - v5
        if rel == "def":
            explained, left = classify_def(root, only6, nodes)
            left += [("v5-only",) + row for row in sorted(only5)]
        elif rel == "call":
            explained, left = classify_call(root, only6, sites)
            left += [("v5-only",) + row for row in sorted(only5)]
        else:
            derive = derive_calls if rel == "calls" else derive_unused
            findings, left = rule_fidelity(rel, derive, sets)
            for finding in findings:
                print(f"  {rel}: RULE FIDELITY — {finding}")
            # Every row of difference on a derived rel is a function of the
            # inputs once BOTH engines are proven to compute v5's rule; the
            # count reported as (a) is therefore the whole difference.
            explained = [] if left else [("rule fidelity holds on both engines",)] * (len(only5) + len(only6))
        rows.append((rel, len(v5), len(v6), len(both), len(only5), len(only6), len(explained), len(left)))
        leftovers += [(rel,) + tuple(item) for item in left]
        if explained and rel in ("def", "call"):
            reasons = {}
            for item in explained:
                reasons[item[-1]] = reasons.get(item[-1], 0) + 1
            for reason, count in sorted(reasons.items()):
                print(f"  {rel}: {count:>4} rows (a) extraction-input difference — {reason}")

    print()
    print("CLASSIFICATION TABLE")
    header = ("rel", "v5", "v6", "match", "v5only", "v6only", "(a)", "(b)+(c)")
    print("  {:<8}{:>7}{:>7}{:>7}{:>8}{:>8}{:>7}{:>9}".format(*header))
    for row in rows:
        print("  {:<8}{:>7}{:>7}{:>7}{:>8}{:>8}{:>7}{:>9}".format(*row))

    if leftovers:
        print()
        print(f"UNCLASSIFIED: {len(leftovers)} rows — bucket (b) or (c), the rig cannot explain these")
        for item in leftovers[:40]:
            print("   ", item)
        return 1
    print()
    print("every difference is bucket (a): 0 rows in (b) v6 expression gap, 0 in (c) real defect")
    return 0


if __name__ == "__main__":
    sys.exit(main())
