#!/usr/bin/env python3
"""Convert a PyCG micro-benchmark case's callgraph.json into our 4-column
call-edge tsv (src_path, src_name, dst_path, dst_name).

Mapping rules (see PYCG-SUITE.md for the full document):
  1. A PyCG qualname is `<module>.<rest...>` where `<module>` is the
     top-level module name and `<rest>` is a dotted qualname inside it
     (pkg.mod.Class.method where pkg.mod is the module). We resolve the
     module prefix to a FILE, not to a name: per case dir we index every
     .py file as its dotted path without .py (a/b.py -> a.b) and every
     package dir (x/__init__.py) also as the dir's dotted path (x). The
     LONGEST dotted prefix of the qualname that hits the map wins; the
     remainder becomes the name, taken as its LAST segment: PyCG keeps
     the Class path in the qualname (main.MyClass.func1) while our arm
     resolves callees to bare function names, so the class prefix is
     dropped on both sides of the comparison. The fuzzy columns of
     SCORES.tsv carry the residual class-ambiguity.
  2. A qualname whose first segment starts with `<` other than `<lambdaN>`
     (e.g. `<builtin>`, `<**PyStr**>`) has no file in the suite: it maps
     to dst_path=<external> with dst_name = the full original qualname.
     These rows are EXCLUDED from recall denominators when scoring.
  3. A pure module node (e.g. src `main`, or dst `nested.mod` when the
     callgraph points at the module's own body) maps to name = "" —
     module-level code, which our resolver reports as caller_name null.
  4. `<lambdaN>` segments are real lambda qualnames inside a file; they
     stay internal with the `<lambdaN>` segment kept verbatim.
  5. Rows are deduplicated; path fields are the file's path relative to
     the case dir, prefixed by the category/case prefix so they line up
     with the paths we hand `extract --resolve` (run from the suite root).

Usage:
  pycg_convert.py CASE_DIR PREFIX     # PREFIX like "classes/call"
"""

import json
import sys
from pathlib import Path

EXTERNAL = "<external>"


def module_map(case_dir: Path):
    """Dotted module -> file path relative to case_dir."""
    mods = {}
    for py in sorted(case_dir.rglob("*.py")):
        rel = py.relative_to(case_dir).with_suffix("")
        dotted = ".".join(rel.parts)
        mods[dotted] = str(rel) + ".py"
        if rel.parts[-1] == "__init__":
            dir_dotted = ".".join(rel.parts[:-1])
            if dir_dotted:
                mods.setdefault(dir_dotted, str(rel) + ".py")
    return mods


def split_qualname(qualname: str, mods):
    """-> (file_path, name) or (EXTERNAL, qualname)."""
    segs = qualname.split(".")
    for i in range(len(segs) - 1, 0, -1):
        mod = ".".join(segs[:i])
        if mod in mods:
            rest = segs[i:]
            return mods[mod], rest[-1]
    if qualname in mods:
        return mods[qualname], ""
    return EXTERNAL, qualname


def convert(case_dir: Path, prefix: str):
    mods = module_map(case_dir)
    graph = json.loads((case_dir / "callgraph.json").read_text())
    rows = set()
    for src, callees in graph.items():
        for dst in callees:
            sp, sn = split_qualname(src, mods)
            dp, dn = split_qualname(dst, mods)
            rows.add(f"{prefix}/{sp}\t{sn}\t{prefix}/{dp}\t{dn}")
    return sorted(rows)


def main():
    case_dir = Path(sys.argv[1])
    prefix = sys.argv[2]
    for row in convert(case_dir, prefix):
        print(row)


if __name__ == "__main__":
    main()
