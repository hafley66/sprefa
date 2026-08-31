#!/usr/bin/env python3
"""ts gap classifier, v2. Joins sample rows against the raw resolve jsonl to
get exact site spans, scans the receiver expression backwards, classifies by
declaration annotation. Buckets (priority order):
  this-in-namespace | call-result receiver (one hop / multi-hop) |
  namespace-merged or imported-module receiver | union receiver |
  func-typed param/field | generic receiver | interface receiver |
  overload group callee | unannotated receiver | other
Usage: classify2.py <raw.jsonl> <sample.tsv|unresolved.jsonl> <mode: sample|unresolved> <out.jsonl>"""
import json
import pathlib
import re
import sys
from collections import Counter

CORPUS = pathlib.Path("/Users/chrishafley/projects/TypeScript-5.9")
IFACE_RE = re.compile(r"^(?:export )?(?:declare )?interface (\w+)", re.M)
FN_RE = re.compile(r"(?:^|\n)(?:export )?(?:default )?(?:async )?function\*? (\w+)")
CLASS_RE = re.compile(r"(?:^|\n)\s*(?:export )?(?:abstract )?class (\w+)")
NS_RE = re.compile(r"(?:^|\n)(?:export )?namespace (\w+)")
TYPEPARAMS_RE = re.compile(r"<([^<>{};]+)>")


DEFS = {}


def corpus_index():
    ifaces = set()
    pat = re.compile(r"(?:^|\n)(?:export )?(?:default )?(?:async )?function\*? (\w+)\b"
                     r"|(?:^|\n)(?:export )?const (\w+)\s*[=:]")
    for p in CORPUS.rglob("*.ts"):
        if p.name.endswith(".d.ts"):
            continue
        text = p.read_text(errors="replace")
        ifaces.update(IFACE_RE.findall(text))
        for a, b in pat.findall(text):
            DEFS[a or b] = DEFS.get(a or b, 0) + 1
    return ifaces


def enclosing_kind(text, idx):
    """Nearest of class / namespace / function before idx."""
    best = None
    for rx, kind in ((CLASS_RE, "class"), (NS_RE, "namespace"), (FN_RE, "function")):
        m = None
        for m2 in rx.finditer(text, 0, idx):
            m = m2
        if m and (best is None or m.start() > best[0]):
            best = (m.start(), kind, m.group(1))
    return best


def recv_of(text, start):
    """Receiver expression ending right before the callee span start."""
    start = min(start, len(text))
    i = start - 1
    while i >= 0 and i < len(text) and text[i] in ".?":
        i -= 1
    depth = 0
    end = i + 1
    while i >= 0:
        c = text[i]
        if c in ")]":
            depth += 1
        elif c in "([":
            if depth == 0:
                break
            depth -= 1
        elif depth == 0 and not (c.isalnum() or c in "_$!"):
            break
        i -= 1
    return text[i + 1:end].strip()


DECL_CACHE = {}

def decl_annot(text, name, idx):
    """Annotation for `name`: nearest const/param/field decl before idx."""
    pats = DECL_CACHE.get(name)
    if pats is None:
        e = re.escape(name)
        pats = [
            (re.compile(r"(?:const|let|var)\s+" + e + r"\s*(?::\s*([^;=]+?))?\s*[=;]"), "const"),
            (re.compile(r"[(,]\s*(?:\.\.\.\s*)?" + e + r"\s*(?:\?)?:\s*([^,)=;]+)"), "param"),
            (re.compile(r"\n\s+(?:public |private |protected |readonly |static )*"
                        + e + r"\s*(?:\?)?:\s*([^;={\n]+)[;=]"), "field"),
        ]
        DECL_CACHE[name] = pats
    cands = []
    for rx, kind in pats:
        for m in rx.finditer(text):
            cands.append((m.start(), m.group(1), kind))
    cands = [c for c in cands if c[0] < idx]
    return cands[-1] if cands else None


def type_params_of(text, idx):
    tps = set()
    enc = enclosing_kind(text, idx)
    if not enc:
        return tps
    st, kind, name = enc
    hdr = text[st:st + 500]
    for m in TYPEPARAMS_RE.finditer(hdr):
        for part in m.group(1).split(","):
            tps.add(part.strip().split(" ")[0].split("=")[0].split(" extends ")[0].strip())
    if kind == "function":
        cstart = None
        for m in CLASS_RE.finditer(text, 0, st):
            cstart = m
        if cstart:
            for m in TYPEPARAMS_RE.finditer(text[cstart.start():cstart.start() + 400]):
                for part in m.group(1).split(","):
                    tps.add(part.strip().split(" ")[0].split("=")[0].split(" extends ")[0].strip())
    return tps


NS_BRACE_RE = re.compile(r"import\s*\{([^}]*?)\}\s*from\s*\S*namespaces")

def imported_names(text):
    """(namespace imports, brace imports from _namespaces barrels)."""
    ns = set()
    for m in re.finditer(r"import\s+(?:type\s+)?\*\s+as\s+(\w+)", text):
        ns.add(m.group(1))
    for m in re.finditer(r"import\s+(?:type\s+)?(\w+)\s*=\s*require", text):
        ns.add(m.group(1))
    brace = set()
    for m in NS_BRACE_RE.finditer(text):
        for part in m.group(1).split(","):
            part = part.strip()
            if part:
                brace.add(part.split(" as ")[-1].strip())
    return ns, brace


def bare_class(callee):
    """Bare call site: how many corpus defs `callee` has."""
    n = DEFS.get(callee, 0)
    if n > 1:
        return "bare call, name ambiguous across files"
    if n == 0:
        return "bare call, def not found (method ref / property fn)"
    return "bare call, single def"


def classify(text, span, callee, ifaces):
    start, end = span
    # the span may cover the whole `recv.member` expr; the callee sits at its end
    if end - len(callee) >= 0 and text[end - len(callee):end] == callee:
        start = end - len(callee)
    recv = recv_of(text, start)
    f = {"recv": recv[:100], "line": ""}
    ls = text.rfind("\n", 0, start) + 1
    le = text.find("\n", start)
    f["line"] = text[ls:le if le > 0 else len(text)].strip()[:130]
    if "?." in text[max(ls, start - 80):start]:
        f["optchain"] = True
    if not recv:
        prev = text[start - 1] if 0 < start < len(text) else ""
        if prev not in ".":
            return "BARE: " + bare_class(callee), f
        return "MANUAL(no-recv)", f
    base = recv.split("(")[0].split(".")[0].split("[")[0].strip().rstrip("!")
    enc = enclosing_kind(text, start)

    if recv in ("this", "super"):
        if enc and enc[1] == "class":
            return "other: this/super in class still unbound", f
        return "this in a namespace / function-style module", f

    if "(" in recv:
        after = recv.split(")")[-1]
        hops = recv.count(").")
        if hops >= 1 and after.count(".") >= 1:
            return "receiver from a call result, more than one hop", f
        return "receiver from a call result (one hop)", f

    ns_imp, brace_imp = imported_names(text)
    if base in ns_imp or base in brace_imp or base == "ts":
        return "method on a namespace-merged / imported-module declaration", f

    d = decl_annot(text, base, start)
    if d is None:
        return "MANUAL(no-decl)", f
    off, ann, kind = d
    f["decl_kind"], f["ann"] = kind, (ann or "")[:90]
    if ann is None:
        return "unannotated receiver (type inferred from initializer)", f
    ann = ann.strip()
    if "|" in ann:
        return "union-typed receiver", f
    if "=>" in ann or ann in ("Function",) or ann.startswith("("):
        return "callback / func-typed param or field", f
    bare = ann.rstrip("[]").strip().split(".")[-1].split("<")[0].strip()
    if bare in type_params_of(text, start):
        return "generic receiver (T extends X)", f
    if bare in ifaces:
        return "interface receiver (needs implementer fan-out)", f
    return "other: declared " + bare[:40], f


def site_by_scan(text, src_name, callee):
    """Fallback: find `callee(` inside the caller fn; return its callee span."""
    if text is None or not callee:
        return None
    pat = re.compile(r"(?:\.|\?\.)?" + re.escape(callee) + r"\s*\(")
    best = None
    for m in pat.finditer(text):
        if src_name and src_name != "<module>":
            enc = enclosing_kind(text, m.start())
            fname = enc[2] if enc else None
            if src_name.endswith(".constructor") and enc and enc[1] == "class" \
               and src_name.split(".")[0] == fname:
                pass
            elif fname != src_name:
                continue
        best = m
        break
    if best is None:
        m = pat.search(text)
        if not m:
            return None
        best = m
    st = best.start() + (1 if best.group(0)[0] in ".?" else 0)
    return (st, st + len(callee))


def main():
    raw, inp, mode, outp = sys.argv[1:5]
    ifaces = corpus_index()
    texts = {}

    def text_of(p):
        if p not in texts:
            fp = CORPUS / p
            texts[p] = fp.read_bytes().decode("utf-8", errors="replace") if fp.exists() else None
        return texts[p]

    # site spans from raw jsonl, keyed by the 4-tuple
    spans = {}
    for line in open(raw):
        d = json.loads(line)
        if d["record"] == "resolved_edge":
            spans.setdefault((d["caller_path"], d["caller_name"], d["callee_path"], d["callee_name"]),
                             (d["caller_site_start"], d["caller_site_end"]))

    out = []
    counter = Counter()
    for line in open(inp):
        line = line.rstrip("\n")
        if not line:
            continue
        if mode == "sample":
            s, sn, d, dn = line.split("\t")
            span = spans.get((s, sn, d, dn))
            if span is None:
                span = site_by_scan(text_of(s), sn, dn)
        else:
            r = json.loads(line)
            s, dn, span = r["path"], r["detail"], (r["span"]["start"], r["span"]["end"])
            sn, d = "", ""
        text = text_of(s)
        if text is None:
            cls, f = "MANUAL(no-file)", {}
        elif span is None:
            cls, f = "MANUAL(no-span)", {}
        else:
            cls, f = classify(text, span, dn, ifaces)
        if mode == "unresolved":
            cls = f'{r["reason"]}: {cls}'
        counter[cls] += 1
        out.append(json.dumps({"class": cls, "src": f"{s} {sn}", "dst": f"{d} {dn}", **f}))

    with open(outp, "w") as fh:
        fh.write("\n".join(out) + "\n")
    for cls, n in counter.most_common():
        print(f"{n}\t{cls}", file=sys.stderr)


if __name__ == "__main__":
    main()
