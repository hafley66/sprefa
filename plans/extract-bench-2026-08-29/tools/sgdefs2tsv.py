#!/usr/bin/env python3
"""Parse `tree-sitter-stack-graphs-typescript query definition` output to module tsv.

usage: sgdefs2tsv.py <corpus_root> <out.tsv> < sg_defs.out
Feeds on the output of tools/ts_import_positions.py positions. Only references
whose resolution lands on a definition inside <corpus_root>/src are emitted.
Columns: src_path src_name dst_path dst_name (names empty for module family).
"""
import os, re, sys

root = os.path.abspath(sys.argv[1])
out = sys.argv[2]
src_prefix = os.path.join(root, "src") + os.sep
ref_pat = re.compile(r"^(.+?):(\d+):(\d+): found \d+ definitions")
def_pat = re.compile(r"^(.+?):\d+:\d+:")
rows = 0
with open(out, "w") as f:
    pending_ref = None
    in_def = False
    for line in sys.stdin:
        m = ref_pat.match(line)
        if m:
            pending_ref = m.group(1)
            in_def = False
            continue
        if line.startswith("has definition") and pending_ref:
            in_def = True
            continue
        if in_def and pending_ref:
            m2 = def_pat.match(line)
            if m2:
                dst = os.path.abspath(m2.group(1))
                src = os.path.abspath(pending_ref)
                pending_ref = None
                in_def = False
                if dst.startswith(src_prefix) and src.startswith(src_prefix):
                    f.write(
                        f"src/{os.path.relpath(src, src_prefix)}\t\t"
                        f"src/{os.path.relpath(dst, src_prefix)}\t\n"
                    )
                    rows += 1
print(rows)
