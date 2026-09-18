import json, os, re, sys
OPS = {f"int_{c}": ("int", c) for c in ["add", "lt", "le", "eq", "ne", "ge", "gt"]}
OPS["term_lt"] = ("any", "lt")
TEXT = re.compile(r"\b(int_(add|lt|le|eq|ne|ge|gt)|term_lt)\b")

def text_sub(s):
    return TEXT.sub(lambda m: ".".join(OPS[m.group(1)]), s)

def atom(t):
    return isinstance(t, dict) and set(t) == {"a"} and t["a"] in OPS

class Rewriter:
    def __init__(self):
        self.fresh = {}   # json(reader_node) -> [new reader_node x3]
        self.changed = 0

    def node_ids(self, rn):
        key = json.dumps(rn, sort_keys=True)
        if key not in self.fresh:
            file, n = rn["args"]
            self.fresh[key] = (rn, [{"f": "reader_node", "args": [file, 1_000_000 + n * 3 + k]} for k in range(3)])
        return self.fresh[key][1]

    def walk(self, t):
        if isinstance(t, list):
            return [self.walk(x) for x in t]
        if isinstance(t, str):
            return t
        if not isinstance(t, dict):
            return t
        if "f" in t:
            f, args = t["f"], t.get("args", [])
            if f == "kernel" and len(args) == 1 and atom(args[0]):
                owner, label = OPS[args[0]["a"]]
                self.changed += 1
                return {"f": "kernel", "args": [{"a": owner}, {"a": label}]}
            if f == "name" and len(args) == 2 and atom(args[1]):
                owner, label = OPS[args[1]["a"]]
                self.changed += 1
                inner = {"f": "name", "args": [self.walk(args[0]), {"a": owner}]}
                return {"f": "name", "args": [inner, {"a": label}]}
            if (f == "node" and len(args) == 2 and isinstance(args[1], dict)
                    and args[1].get("f") == "atom" and len(args[1]["args"]) == 1 and atom(args[1]["args"][0])):
                owner, label = OPS[args[1]["args"][0]["a"]]
                ids = self.node_ids(args[0])
                self.changed += 1
                kids = [{"f": "node", "args": [i, {"f": "atom", "args": [{"a": a}]}]} for i, a in zip(ids, [".", owner, label])]
                return {"f": "node", "args": [args[0], {"f": "form", "args": [kids]}]}
            return {**t, "args": [self.walk(a) for a in args]}
        return {k: self.walk(v) for k, v in t.items()}

    def source_rows(self, rows):
        out = []
        for row in rows:
            out.append(row)
            if isinstance(row, dict) and row.get("f") == "source":
                key = json.dumps(row["args"][0], sort_keys=True)
                if key in self.fresh:
                    for i in self.fresh[key][1]:
                        out.append({**row, "args": [i] + row["args"][1:]})
        return out

def fix_sources(t, rw):
    if isinstance(t, dict):
        return {k: (rw.source_rows(v) if k == "source_rows" and isinstance(v, list) else fix_sources(v, rw)) for k, v in t.items()}
    return t

def strings(t):
    if isinstance(t, str):
        return text_sub(t)
    if isinstance(t, list):
        return [strings(x) for x in t]
    if isinstance(t, dict):
        return {k: strings(v) for k, v in t.items()}
    return t

total = 0
for root, _, files in os.walk("oracle"):
    for f in files:
        p = os.path.join(root, f)
        raw = open(p).read()
        if not TEXT.search(raw):
            continue
        if f.endswith(".dl7"):
            open(p, "w").write(text_sub(raw))
            print("text", p); total += 1
            continue
        if not f.endswith(".json"):
            continue
        case = json.loads(raw)
        if not isinstance(case, dict):
            continue
        rw = Rewriter()
        new = {}
        for k, v in case.items():
            if k == "expected":
                new[k] = v
                continue
            v = rw.walk(v)
            # read cases carry source text as a plain string input
            if root.endswith("oracle/read") and k == "input":
                v = strings(v)
            new[k] = v
        new = fix_sources(new, rw) if rw.fresh else new
        if new != case:
            with open(p, "w") as h:
                json.dump(new, h, sort_keys=True, separators=(",", ":"))
            print("json", p, rw.changed, "fresh", len(rw.fresh)); total += 1
print("files", total)
