#!/usr/bin/env python3
"""Convert dependency-cruiser --output-type json to normal-form tsv (module family).

usage: depcruise2tsv.py <corpus_root> <out.tsv> < depcruise.json
Emits dependencies whose `resolved` is a file inside the corpus root.
Columns: src_path src_name dst_path dst_name (names empty for module family).
"""
import json, os, sys

root, out = os.path.abspath(sys.argv[1]), sys.argv[2]
data = json.load(sys.stdin)
rows = 0
with open(out, "w") as f:
    for m in data["modules"]:
        src = os.path.abspath(os.path.join(root, m["source"]))
        if not os.path.isfile(src):
            continue
        s = os.path.relpath(src, root)
        for dep in m["dependencies"]:
            dst = os.path.abspath(os.path.join(root, dep["resolved"]))
            if not os.path.isfile(dst):
                # depcruise resolves ts imports to compiled .js; map back to source files
                base, ext = os.path.splitext(dst)
                for cand in (base + ".ts", base + ".d.ts", base + ".tsx"):
                    if os.path.isfile(cand):
                        dst = cand
                        break
                else:
                    continue
            f.write(f"{s}\t\t{os.path.relpath(dst, root)}\t\n")
            rows += 1
print(rows)
