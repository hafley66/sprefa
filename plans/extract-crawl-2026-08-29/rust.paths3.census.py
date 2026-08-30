#!/usr/bin/env python3
"""Class 8/11/9/7 census over one-process unresolved rows with the throwaway
#recv=<T>#impls=<n> detail tag (section 18's method, lane fix-extract-rust-paths-3).

Usage: rust.paths3.census.py <run.tsv> <corpus-root>
"""
import json, os, re, sys, collections

run_tsv, corpus = sys.argv[1], sys.argv[2].rstrip("/")

# corpus type tables, from the same file list the run covered
structs, enums, traits, aliases = set(), set(), set(), set()
decl_re = re.compile(r"^\s*(?:pub(?:\(.*?\))?\s+)?(struct|enum|trait|type)\s+([A-Z]\w*)", re.M)
for line in open(run_tsv):
    pass
paths = set()
for line in open(run_tsv):
    d = json.loads(line)
    paths.add(d.get("path") or d.get("caller_path") or "")
for p in paths:
    if not p:
        continue
    try:
        text = open(os.path.join(corpus, p)).read()
    except OSError:
        continue
    for m in decl_re.finditer(text):
        kind, name = m.group(1), m.group(2)
        {"struct": structs, "enum": enums, "trait": traits, "type": aliases}[kind].add(name)

STD = {"Vec", "String", "Option", "Result", "Box", "Arc", "Rc", "HashMap", "HashSet",
       "BTreeMap", "BTreeSet", "FxHashMap", "FxHashSet", "str", "usize", "u32", "u64",
       "i32", "i64", "f32", "f64", "bool", "char", "u8", "i8", "Ordering", "Path", "PathBuf"}

counts = collections.Counter()
samples = {}
rows = []
for line in open(run_tsv):
    d = json.loads(line)
    if d.get("record") != "unresolved" or d.get("family") != "call":
        continue
    if d.get("reason") != "ambiguous":
        continue
    detail = d.get("detail", "")
    path, span = d["path"], d["span"]
    src = open(os.path.join(corpus, path), "rb").read()[span["start"]:span["end"]].decode("utf8", "replace")
    m = re.search(r"#recv=([^#]*)#impls=(\d+)", detail)
    tm = re.search(r"#ty=([^#]*)#impls=(\d+)", detail)
    first = src.split("::")[0].split("(")[0].strip().lstrip("<").lstrip("&")
    if tm and (first[:1].isupper() or first == "crate"):
        ty, impls = tm.group(1), int(tm.group(2))
        if ty in STD:
            cls = "10"
        elif ty in structs or ty in enums:
            cls = "8" if impls != 1 else "8bound"
        elif ty in aliases:
            cls = "10a"
        else:
            cls = "10"
    elif "::" in src:
        segs = [s for s in src.split("::") if s]
        first = segs[0].split("(")[0].strip()
        if first and first[0].islower() or first == "crate" or first == "self" or first == "super":
            cls = "11"
        elif first in structs or first in enums:
            cls = "8"
        elif first in aliases:
            cls = "10a"
        else:
            cls = "10"
    elif m:
        recv, impls = m.group(1), int(m.group(2))
        if recv == "?":
            cls = "1"
        elif recv in aliases:
            cls = "3a"
        elif recv in structs or recv in enums:
            if impls >= 2:
                cls = "7"
            elif impls == 0:
                cls = "5"
            else:
                cls = "bound"
        elif recv in traits:
            cls = "6"
        elif recv in STD or not (recv in structs or recv in enums or recv in traits):
            cls = "3b"
        else:
            cls = "3b"
    else:
        cls = "9"
    counts[cls] += 1
    samples.setdefault(cls, f"{path}:{span['start']} {src[:40]!r}")
    rows.append((cls, path, span["start"], src[:60]))

total = sum(counts.values())
print(f"ambiguous call drops total {total}")
for cls in sorted(counts):
    print(f"class {cls:4} {counts[cls]:6}  e.g. {samples[cls]}")
