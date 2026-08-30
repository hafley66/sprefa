#!/usr/bin/env python3
"""Classify the rust excess rows (ours minus one oracle, after the
projection). Classes, checked in order:

  generated        src_path or dst_path carries a generated/ dir or a tracked
                   generated node file.
  macro site       the caller file is a macro-expansion test or fixture
                   (macro_expansion_tests, test_data, fixtures).
  wrong target     the oracle has a different def for the same
                   (src_path, src_name, dst_name) triple: same callee name,
                   other def.
  trait fan-out    the callee name is declared in a corpus trait fn list
                   (a `fn <name>` inside a `trait` block) and our dst is an
                   impl block file: we picked one implementer where the
                   oracle keeps the site.
  method value     dst_name appears in the caller file only as a value
                   (passed as `Name(` / `SomeType::Name` without `(`).
  other

usage: classify_excess.py <excess.tsv> <oracle.tsv> <corpus_root>
"""
import os
import re
import sys


def load(path):
    return [line.rstrip("\n") for line in open(path) if line.strip()]


def trait_fns_in_text(text):
    # Every `trait X { .. }` block: fn names declared or defaulted inside.
    fns = set()
    for m in re.finditer(r"\btrait\s+[A-Za-z_]", text):
        open_brace = text.find("{", m.start())
        if open_brace == -1:
            continue
        depth = 0
        end = open_brace
        for i in range(open_brace, len(text)):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    end = i
                    break
        for fn in re.finditer(r"\bfn\s+([a-zA-Z_][a-zA-Z0-9_]*)", text[m.start():end]):
            fns.add(fn.group(1))
    return fns


def trait_fns(root):
    fns = set()
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in ("target", ".git")]
        for name in filenames:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            try:
                text = open(path, encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            fns |= trait_fns_in_text(text)
    return fns


def main():
    excess_path, oracle_path, corpus_root = sys.argv[1], sys.argv[2], sys.argv[3]
    oracle = set(load(oracle_path))
    traits = trait_fns(corpus_root)
    classes = {}
    examples = {}
    for row in load(excess_path):
        src_path, src_name, dst_path, dst_name = row.split("\t")
        if "generated" in src_path or "generated" in dst_path or "test_data" in dst_path:
            kind = "generated"
        elif "macro_expansion_tests" in src_path or "test_data" in src_path or "fixtures" in src_path:
            kind = "macro site"
        elif any(
            o.split("\t")[0] == src_path
            and o.split("\t")[1] == src_name
            and o.split("\t")[3] == dst_name
            and o.split("\t")[2] != dst_path
            for o in oracle
        ):
            kind = "wrong target"
        elif dst_name in traits:
            kind = "trait fan-out"
        else:
            kind = "other"
        classes[kind] = classes.get(kind, 0) + 1
        examples.setdefault(kind, []).append(row)
    for kind in sorted(classes, key=lambda k: -classes[k]):
        print(f"{kind}\t{classes[kind]}")
        for row in examples[kind][:4]:
            print(f"  {row}")


if __name__ == "__main__":
    main()
