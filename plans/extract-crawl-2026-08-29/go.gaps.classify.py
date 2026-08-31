#!/usr/bin/env python3
"""One-pass gap classifier. Reads sample tsv (src_path src_name dst_path dst_name),
emits (dst_name, class) per row. Usage: classify.py <sample.tsv>."""
import re
import sys
import collections
import pathlib

CORPUS = pathlib.Path("/Users/chrishafley/projects/typescript-go")
recv_re = re.compile(r"^func \((\w+ )?\*?(\w+)\) (\w+)\(")
iface_re = re.compile(r"^\t(\w+)\(")


def load_defs():
    defs = collections.defaultdict(set)
    ifaces = collections.defaultdict(set)
    for p in CORPUS.rglob("*.go"):
        d = str(p.parent.relative_to(CORPUS))
        in_iface = False
        for line in p.read_text(errors="replace").splitlines():
            s = line.strip()
            if s.startswith("type ") and s.endswith("interface {"):
                in_iface = True
                continue
            if in_iface:
                if s == "}":
                    in_iface = False
                else:
                    m = iface_re.match(line)
                    if m:
                        ifaces[d].add(m.group(1))
                continue
            if s.startswith("func "):
                m = recv_re.match(s)
                if m:
                    defs[m.group(3)].add((d, m.group(2)))
    return defs, ifaces


def classify(row, defs, ifaces):
    src_path, src_name, dst_path, dst_name = row.split("\t")
    if re.search(r"\$\d+$", dst_name):
        return dst_name, "vta-closure-callee (func literal named by vta; we drop)"
    p = CORPUS / src_path
    if not p.exists():
        return dst_name, "MANUAL(no-file)"
    text = p.read_text(errors="replace").splitlines()
    fstarts = []
    for i, l in enumerate(text):
        if l.startswith("func "):
            m = re.search(r"func (?:\([^)]*\) )?([\w$]+)", l)
            if m:
                fstarts.append((i, m.group(1)))
    enc = None
    if "$" in src_name:
        base = src_name.split("$")[0]
    else:
        base = src_name
    sites = []
    pat = re.compile(r"\." + re.escape(dst_name) + r"\(|\b" + re.escape(dst_name) + r"\(")
    for i, line in enumerate(text):
        if pat.search(line):
            fn = None
            for j in range(len(fstarts) - 1, -1, -1):
                if fstarts[j][0] <= i:
                    fn = fstarts[j][1]
                    break
            if fn == base:
                sites.append((i + 1, line.strip()))
    if "$" in src_name:
        if sites:
            return dst_name, "closure-caller naming (vta attrs to fn$N; we mirror to fn)"
        return dst_name, "MANUAL(closure-caller, no site)"
    ndefs = len(defs.get(dst_name, ()))
    iface = "y" if dst_name in ifaces.get(str(pathlib.PurePosixPath(dst_path).parent), ()) else "n"
    for ln, t in sites:
        if re.search(r"\.\w+\(\)\.\w+\(", t):
            return dst_name, "multi-hop receiver chain a.b().c() (iface on hop)"
    for ln, t in sites:
        if not re.search(re.escape(dst_name) + r"\(", t):
            return dst_name, "method value / func-typed field or param"
    if ndefs > 1 or iface == "y":
        return dst_name, "interface method dispatch"
    if not sites:
        return dst_name, "MANUAL(no site in caller)"
    return dst_name, "MANUAL(direct single-def)"


def main():
    defs, ifaces = load_defs()
    rows = [l.rstrip("\n") for l in open(sys.argv[1]) if l.strip()]
    from collections import Counter
    c = Counter()
    examples = {}
    for row in rows:
        name, cls = classify(row, defs, ifaces)
        c[cls] += 1
        examples.setdefault(cls, []).append(row)
    for cls, n in c.most_common():
        print(f"{n}\t{cls}")
    print("=== examples ===")
    for cls, ex in examples.items():
        print(f"-- {cls}")
        for r in ex[:4]:
            print("   " + r.replace("\t", " | "))


if __name__ == "__main__":
    main()
