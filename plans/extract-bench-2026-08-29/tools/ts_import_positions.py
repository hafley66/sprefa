#!/usr/bin/env python3
"""Emit import-specifier reference positions (PATH:LINE:COL) for a TS source tree.

These feed `tree-sitter-stack-graphs-typescript query definition` to get
name-binding resolutions from stack graphs.
"""
import os, re, sys

root = sys.argv[1]
pat = re.compile(r"""(?:from|import)\s*['"]([^'"]+)['"]""")
for dirpath, _dirs, files in os.walk(os.path.join(root, "src")):
    for name in files:
        if not name.endswith((".ts", ".tsx")):
            continue
        path = os.path.join(dirpath, name)
        with open(path, encoding="utf8", errors="replace") as f:
            for lineno, line in enumerate(f, 1):
                for m in pat.finditer(line):
                    col = m.start(1) + 1
                    print(f"{path}:{lineno}:{col}")
