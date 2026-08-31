#!/usr/bin/env python3
"""Classify the rust call leg's miss set against the CodeQL oracle.

The ratchet row RATCHET.tsv line 8 (rust call vs rust.codeql.call.tsv,
checker tier) names a ~26% miss set; this script is its reproducible census.
Three inputs:

  ours    RUST_CALL_DUMP=<path> from tests/79_rust_type_dump.rs
  oracle  rust.codeql.call.tsv (default)
  root    the corpus checkout the paths are relative to

Both sides run through the ratchet projection first (the Rust port in
tests/bench/mod.rs `rust_project`: the oracle drops dst-out-of-corpus rows,
ours drops callers the oracle never calls from, both drop mirrored
`closure@<n>` rows). Misses are the projected oracle minus projected ours;
ours-only rows split contradicted (the oracle judges that caller pair
differently) vs unjudged (no oracle row shares the caller).

A miss is classified from the two source files the row names: where the
callee is spelled in the caller file (plain text, inside macro-invocation
arguments, absent) and what kind of item declares it in the dst file (free
fn, trait method, impl method, macro-minted).

    python3 rust.call_census.py <ours> [--oracle F] [--root D]
                               [--examples N] [--class C]
"""

import argparse
import collections
import json
import os
import pathlib
import random
import re
import sys

BENCH = pathlib.Path(__file__).resolve().parent
DEFAULT_ORACLE = BENCH / "rust.codeql.call.tsv"
DEFAULT_ROOT = "/Users/chrishafley/projects/rust-analyzer"


def read_rows(path, cols=4):
    out = []
    with open(path) as handle:
        for line in handle:
            line = line.rstrip("\n")
            if not line:
                continue
            parts = line.split("\t")
            parts += [""] * (cols - len(parts))
            out.append(tuple(parts[:cols]))
    return out


def corpus_files(root):
    """tests/bench/mod.rs `wants`: crates/*/**/src/**/*.rs."""
    found = set()
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [
            d for d in dirnames
            if not d.startswith(".") and d not in ("target", "node_modules")
        ]
        for name in filenames:
            if not name.endswith(".rs"):
                continue
            rel = os.path.relpath(os.path.join(dirpath, name), root)
            parts = rel.split("/")
            if parts[0] == "crates" and len(parts) > 2 and "src" in parts[1:]:
                found.add(rel)
    return found


# ── the ratchet projection (tests/bench/mod.rs rust_project, ported) ────────

def closure_enclosing(rows):
    """A `closure@<n>` row drops when a non-closure row shares its
    (src_path, dst_path, dst_name) triple."""
    plain = {
        (s, d, n)
        for s, fn, d, n in rows
        if not fn.startswith("closure@")
    }
    return {
        row for row in rows
        if not row[1].startswith("closure@") or (row[0], row[2], row[3]) not in plain
    }


def rust_project(ours, oracle, files):
    oracle_scoped = {row for row in oracle if row[2] in files}
    oracle_srcs = {row[0] for row in oracle_scoped}
    ours_scoped = {row for row in ours if row[0] in oracle_srcs}
    return closure_enclosing(ours_scoped), closure_enclosing(oracle_scoped)


# ── source-text facts ───────────────────────────────────────────────────────

def word_in(text, word):
    return re.search(rf"\b{re.escape(word)}\b", text) is not None


OPEN, CLOSE = "({[", ")}]"


def macro_lines(text):
    """1-indexed line numbers whose text sits inside a `name!(...)` argument
    list: from the line a macro invocation opens to the line its bracket
    closes. Cheap and sufficient for classification, not a parser."""
    inside = set()
    line = 1
    stack = []  # is-macro flag per open bracket
    for pos, char in enumerate(text):
        if char == "\n":
            line += 1
        elif char in OPEN:
            i = pos - 1
            while i >= 0 and text[i] in " \t\r\n":
                i -= 1
            j = i
            while j >= 0 and (text[j].isalnum() or text[j] in "_!"):
                j -= 1
            stack.append(text[j + 1 : i + 1].endswith("!"))
        elif char in CLOSE and stack:
            if stack.pop() or any(stack):
                inside.add(line)
        if stack and any(stack):
            inside.add(line)
    return inside


def lines_naming(text, word):
    """1-indexed lines where `word` appears as a whole word."""
    pat = re.compile(rf"\b{re.escape(word)}\b")
    out = []
    for num, line in enumerate(text.splitlines(), 1):
        if pat.search(line):
            out.append(num)
    return out


ITEM_HEAD = {
    "trait": re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?(auto\s+)?trait\s+([A-Za-z_]\w*)"),
    "impl": re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?impl\b"),
    "macro": re.compile(r"^\s*macro_rules!\s+([A-Za-z_]\w*)"),
    "fn": re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]*\"\s+)?fn\s+([A-Za-z_]\w*)"),
}


def decl_kinds(text, name):
    """The item kinds `name` is declared as in this file: free fn, trait
    method (with the trait name), impl method, macro-minted (declared inside
    a macro_rules body), as a set. Heuristic, rustfmt-shaped: a container
    opened on the fn's own line owns the fn (`trait T { fn m(); }`); one that
    closed before it does not. A wrapped `impl ... where ...` defers to the
    line its `{` opens."""
    fn_pat = re.compile(rf"\bfn\s+{re.escape(name)}\b")
    stack = []  # (kind, name, brace depth the body closes at)
    pending = None  # a header seen without its `{` yet
    depth = 0
    kinds = set()
    for line in text.splitlines():
        if line.lstrip().startswith("//"):
            continue
        pushed = None
        for kind, pat, group in (
            ("trait", ITEM_HEAD["trait"], 2),
            ("impl", ITEM_HEAD["impl"], None),
            ("macro", ITEM_HEAD["macro"], 1),
        ):
            hit = pat.match(line)
            if hit:
                pending = (
                    kind,
                    hit.group(group) if group and hit.groups() else "",
                )
                break
        before = depth
        depth += line.count("{") - line.count("}")
        hit = fn_pat.search(line)
        if hit:
            brace = line.find("{")
            owner = None
            if brace != -1 and brace < hit.start() and pending:
                owner = pending
            elif stack:
                owner = stack[-1]
            if owner is None:
                kinds.add("free fn")
            elif owner[0] == "trait":
                kinds.add(f"trait {owner[1]}")
            elif owner[0] == "impl":
                kinds.add("impl method")
            else:
                kinds.add("macro-minted")
        if pending and depth > before:
            stack.append((pending[0], pending[1], depth))
            pending = None
        while stack and depth < stack[-1][2]:
            stack.pop()
    return kinds


SUGAR = {
    "clone", "eq", "ne", "hash", "partial_cmp", "cmp", "next", "iter",
    "into_iter", "from_iter", "deref", "deref_mut", "drop", "from", "into",
    "add", "sub", "mul", "div", "rem", "not", "neg", "bitand", "bitor",
    "bitxor", "shl", "shr", "le", "ge", "lt", "gt", "fmt", "to_string",
    "borrow", "borrow_mut", "default", "index", "index_mut",
    "call", "call_mut", "call_once",
}


def classify(row, texts, macros, kinds_of, ours_index, files):
    src, caller, dst_file, callee = row
    # ours-side joins first: a row we emit under a different spelling is a
    # naming disagreement, not a missing site, and needs no source read.
    if ours_index.closure_hits(src, dst_file, callee):
        return "C1 ours emits the edge under a `closure@<n>` caller"
    if ours_index.same_callee_elsewhere(src, caller, callee, dst_file):
        return "A1 ours emits the callee aimed at a different dst"
    if ours_index.same_dst_other_name(src, caller, dst_file, callee):
        if callee in SUGAR:
            return "S1 operator/derive sugar, ours re-spells at the same dst"
        return "A2 ours reaches the dst under a different callee name"
    src_text = texts(src)
    if src not in files:
        return "X1 caller file outside the corpus"
    if not word_in(src_text, caller):
        return "M1 macro-minted caller (name absent from the caller file)"
    callee_lines = lines_naming(src_text, callee)
    sugar = callee in SUGAR
    if not callee_lines:
        return "S2 operator/derive sugar, name never spelled" if sugar else \
            "M2 callee name absent from the caller file"
    if all(num in macros(src) for num in callee_lines):
        return "M3 callee spelled only inside macro-invocation arguments"
    kinds = kinds_of(dst_file, callee)
    absent = ours_index.caller_row_count(src, caller) == 0
    lone = ", no ours row from this caller" if absent else ", ours rows disjoint"
    if kinds & {"macro-minted"}:
        return "M4 callee is itself macro-minted in the dst file"
    if sugar:
        return "S3 operator/derive sugar, dst names the trait/impl"
    if kinds:
        if any(k.startswith("trait ") for k in kinds):
            return "T1 trait default method" + lone
        if "impl method" in kinds:
            return "T2 impl method" + lone
        if "free fn" in kinds:
            return "F1 free fn, unresolved" + lone
        return "O1 " + "/".join(sorted(kinds))
    return "O2 callee has no `fn` declaration in the dst file" + lone


class OursIndex:
    """The ours-side joins a miss can hit: the same edge under a
    `closure@<n>` caller, the same callee aimed at a different dst, the same
    dst reached under a different callee name, and whether the caller names
    any row of ours at all."""

    def __init__(self, ours):
        self.closure = collections.defaultdict(set)
        self.by_callee = collections.defaultdict(set)
        self.by_dst = collections.defaultdict(set)
        self.by_caller = collections.defaultdict(set)
        for src, caller, dst, name in ours:
            if caller.startswith("closure@"):
                self.closure[(src, dst, name)].add(caller)
            self.by_callee[(src, caller, callee := name)].add(dst)
            self.by_dst[(src, caller, dst)].add(name)
            self.by_caller[(src, caller)].add((dst, name))

    def closure_hits(self, src, dst, callee):
        return bool(self.closure.get((src, dst, callee)))

    def caller_row_count(self, src, caller):
        return len(self.by_caller.get((src, caller), ()))

    def same_callee_elsewhere(self, src, caller, callee, dst):
        others = self.by_callee.get((src, caller, callee), set()) - {dst}
        if others:
            return f"{len(others)} dst(s), e.g. {sorted(others)[0]}"
        return ""

    def same_dst_other_name(self, src, caller, dst, callee):
        others = self.by_dst.get((src, caller, dst), set()) - {callee}
        if others:
            return f"{len(others)} name(s), e.g. {sorted(others)[0]}"
        return ""


def locate(root, rel, name, cache):
    """The first line naming `name` in `rel`, for the example rows."""
    if rel not in cache:
        try:
            cache[rel] = open(os.path.join(root, rel)).read()
        except OSError:
            cache[rel] = ""
    for num, line in enumerate(cache[rel].splitlines(), 1):
        if re.search(rf"\b{re.escape(name)}\b", line):
            return num
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ours")
    ap.add_argument("--oracle", default=str(DEFAULT_ORACLE))
    ap.add_argument("--root", default=DEFAULT_ROOT)
    ap.add_argument("--examples", type=int, default=5)
    ap.add_argument("--class", dest="only", default=None)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument(
        "--drops",
        default=None,
        help="the full-batch JSONL of `extract --resolve --family call` over "
        "the same file set; prints a per-class drop-reason histogram",
    )
    ap.add_argument(
        "--drops-root",
        default=None,
        help="the root the drop-fact paths are relative to, when it differs "
        "from --root (a corpus copy the diet run pointed at)",
    )
    args = ap.parse_args()

    ours_raw = set(read_rows(args.ours))
    oracle_raw = set(read_rows(args.oracle))
    files = corpus_files(args.root)
    ours, oracle = rust_project(ours_raw, oracle_raw, files)

    overlap = ours & oracle
    missing = oracle - ours
    excess = ours - oracle
    recall = 100.0 * len(overlap) / len(oracle) if oracle else 0.0
    precision = 100.0 * len(overlap) / len(ours) if ours else 0.0

    print(f"oracle {len(oracle)}  ours {len(ours)}  overlap {len(overlap)}  "
          f"missing {len(missing)}  excess {len(excess)}")
    print(f"recall {recall:.2f}  precision {precision:.2f}")
    print(f"corpus files {len(files)}\n")

    oracle_pairs = {(s, fn) for s, fn, _d, _n in oracle}
    contradicted = {row for row in excess if (row[0], row[1]) in oracle_pairs}
    unjudged = excess - contradicted
    print(f"ours-only {len(excess)}: contradicted {len(contradicted)}  "
          f"unjudged {len(unjudged)}\n")

    cache = {}
    macro_cache = {}
    kind_cache = {}
    def texts(rel):
        if rel not in cache:
            try:
                cache[rel] = open(os.path.join(args.root, rel)).read()
            except OSError:
                cache[rel] = ""
        return cache[rel]

    def macros(rel):
        if rel not in macro_cache:
            macro_cache[rel] = macro_lines(texts(rel))
        return macro_cache[rel]

    def kinds_of(rel, name):
        key = (rel, name)
        if key not in kind_cache:
            kind_cache[key] = decl_kinds(texts(rel), name)
        return kind_cache[key]

    buckets = collections.defaultdict(list)
    ours_index = OursIndex(ours)
    for row in sorted(missing):
        cls = classify(row, texts, macros, kinds_of, ours_index, files)
        buckets[cls].append(row)

    drops = collections.defaultdict(list)
    if args.drops:
        prefix = (args.drops_root or args.root).rstrip("/") + "/"
        with open(args.drops) as handle:
            for line in handle:
                try:
                    fact = json.loads(line)
                except ValueError:
                    continue
                if fact.get("record") != "unresolved":
                    continue
                path = fact.get("path", "")
                rel = path[len(prefix):] if path.startswith(prefix) else path
                drops[(rel, fact.get("detail", ""))].append(fact.get("reason", "?"))

    print(f"{'class':<64} {'rows':>6} {'% miss':>7}")
    for name, rows in sorted(buckets.items(), key=lambda kv: -len(kv[1])):
        print(f"{name:<64} {len(rows):>6} {100.0 * len(rows) / len(missing):>6.2f}%")

    def drop_summary(rows):
        """One line per class: the drop-reason histogram over its rows. A row
        with no drop fact for its (file, callee) is `site-not-minted`."""
        if not args.drops:
            return None
        hist = collections.Counter()
        for row in rows:
            reasons = drops.get((row[0], row[3]), ())
            hist.update(reasons) if reasons else hist.update(["site-not-minted"])
        return "  drops: " + "  ".join(f"{k}={v}" for k, v in hist.most_common())

    rng = random.Random(args.seed)
    for name, rows in sorted(buckets.items(), key=lambda kv: -len(kv[1])):
        if args.only and not name.startswith(args.only):
            continue
        print(f"\n== {name} ({len(rows)} rows)")
        summary = drop_summary(rows)
        if summary:
            print(summary)
        for src, caller, dst_file, callee in rng.sample(rows, min(args.examples, len(rows))):
            line = locate(args.root, src, callee, cache) if callee else 0
            print(f"   {src}:{line}  {caller} -> {callee}  [{dst_file}]")

    return 0


if __name__ == "__main__":
    sys.exit(main())
