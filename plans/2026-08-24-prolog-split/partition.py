"""partition.py -- outline a predmap, or grade a proposed cut against it.

  python3 partition.py outline <name>.predmap.json
  python3 partition.py report  cuts/<name>.cuts.json > reports/<name>.md

A cuts file is {"file","folder","module","parts":[{"file","anchor","owns"}]}.
An anchor is the name/arity of the part's FIRST predicate; the part runs to the
term before the next anchor's first clause, so no line number is hand-typed.
"""

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load(path):
    with open(path) as handle:
        return json.load(handle)


def clause_terms(pm):
    return [t for t in pm["terms"] if t["kind"] in ("clause", "dcg")]


def key_of(term):
    return "%s/%s" % (term["name"], term["arity"])


def outline(pm):
    rows = []
    seen = {}
    for term in pm["terms"]:
        if term["kind"] == "directive":
            rows.append((term["start"], term["end"], ":- " + term["name"], 0, ""))
            continue
        key = key_of(term)
        if key in seen:
            seen[key][1] = term["end"]
            seen[key][3] += 1
            continue
        entry = [term["start"], term["end"], key, 1, term["kind"]]
        seen[key] = entry
        rows.append(entry)
    print("%-6s %-6s %-6s %-4s %s" % ("start", "last", "lines", "cl", "predicate"))
    for row in rows:
        start, end, key, count, kind = row
        print("%-6d %-6d %-6d %-4s %s%s"
              % (start, end, end - start + 1, count, key,
                 "  (dcg)" if kind == "dcg" else ""))
    print("\ntotal lines %d, predicates %d, terms %d"
          % (pm["lines"], len(pm["predicates"]), len(pm["terms"])))


def resolve(pm, cuts):
    """Assign every clause term to a part; return parts with ranges and defs."""
    first_line = {}
    for pred in pm["predicates"]:
        first_line[pred["key"]] = pred["first"]

    anchored = [p for p in cuts["parts"] if p.get("anchor")]
    virtual = [p for p in cuts["parts"] if not p.get("anchor")]

    anchors = []
    for part in anchored:
        anchor = part["anchor"]
        if anchor not in first_line:
            raise SystemExit("anchor %s not defined in %s" % (anchor, cuts["file"]))
        anchors.append((first_line[anchor], part))
    anchors.sort(key=lambda pair: pair[0])
    if [a[1] for a in anchors] != anchored:
        raise SystemExit("parts are not listed in file order")

    bounds = []
    for index, (line, part) in enumerate(anchors):
        end = anchors[index + 1][0] - 1 if index + 1 < len(anchors) else pm["lines"]
        bounds.append({"part": part, "start": line, "end": end,
                       "defs": {}, "terms": 0, "calls": set()})
    for part in virtual:
        seat = [b["part"]["file"] for b in bounds].index(part["after"]) + 1
        bounds.insert(seat, {"part": part, "start": None, "end": None,
                             "defs": {}, "terms": 0, "calls": set()})

    floor = min(b["start"] for b in bounds if b["start"])
    head = [t for t in pm["terms"] if t["end"] < floor]
    stray = [t for t in head if t["kind"] != "directive"]
    for term in pm["terms"]:
        if term["kind"] != "directive" or term["end"] < floor:
            continue
        for bound in bounds:
            if bound["start"] and bound["start"] <= term["start"] <= bound["end"]:
                bound.setdefault("inner", []).append(term)
                break

    by_file = {b["part"]["file"]: b for b in bounds}
    moved = {}
    for move in cuts.get("relocations", []):
        lo, hi = move["lines"]
        for line in range(lo, hi + 1):
            moved[line] = move
        target = by_file[move["to"]]
        target["lines_moved_in"] = target.get("lines_moved_in", 0) + (hi - lo + 1)

    for term in clause_terms(pm):
        home = None
        if term["start"] in moved:
            home = by_file[moved[term["start"]]["to"]]
        else:
            for bound in bounds:
                if bound["start"] and bound["start"] <= term["start"] <= bound["end"]:
                    home = bound
                    break
        if home is None:
            continue
        key = key_of(term)
        home["defs"].setdefault(key, 0)
        home["defs"][key] += 1
        home["terms"] += 1
        home["calls"].update(term["calls"])

    for move in cuts.get("relocations", []):
        lo, hi = move["lines"]
        for bound in bounds:
            if bound["start"] and bound["start"] <= lo <= bound["end"]:
                bound["lines_moved_out"] = (bound.get("lines_moved_out", 0)
                                            + (hi - lo + 1))
    return bounds, head, stray, floor


def report(cuts):
    pm = load(HERE / cuts["predmap"])
    bounds, head, stray, floor = resolve(pm, cuts)
    in_file = {p["key"] for p in pm["predicates"]}
    owner = {}
    split = {}
    for bound in bounds:
        for key in bound["defs"]:
            owner.setdefault(key, []).append(bound["part"]["file"])
    for key, files in owner.items():
        if len(files) > 1:
            split[key] = files

    name = cuts["file"]
    print("# %s -> %s/" % (name, cuts["folder"]))
    print()
    print("module head keeps lines 1..%d (%d lines): %d directives, %d stray clauses"
          % (floor - 1, floor - 1, len(head), len(stray)))
    print()
    print("| part | lines | span | clauses | predicates |")
    print("|---|---:|---|---:|---:|")
    total = 0
    for bound in bounds:
        base = (bound["end"] - bound["start"] + 1) if bound["start"] else 0
        size = base + bound.get("lines_moved_in", 0) - bound.get("lines_moved_out", 0)
        bound["size"] = size
        total += size
        span = "%d-%d" % (bound["start"], bound["end"]) if bound["start"] else "relocated"
        if bound.get("lines_moved_in") or bound.get("lines_moved_out"):
            span += " *"
        print("| `%s` | %d | %s | %d | %d |"
              % (bound["part"]["file"], size, span,
                 bound["terms"], len(bound["defs"])))
    print("| **total** | **%d** | | | |" % total)
    print()
    if cuts.get("relocations"):
        print("`*` = the span plus or minus a relocation:")
        print()
        print("| predicate | lines | moves to | lands after |")
        print("|---|---|---|---|")
        for move in cuts["relocations"]:
            print("| `%s` | %d-%d | `%s` | %s |"
                  % (move["predicate"], move["lines"][0], move["lines"][1],
                     move["to"], move["after"]))
        print()
    over = [b for b in bounds if b["size"] > 700]
    print("parts over 700 lines: %s"
          % (", ".join(b["part"]["file"] for b in over) if over else "none"))
    print()
    print("## clauses of one predicate landing in two parts")
    print()
    if split:
        print("| predicate | parts |")
        print("|---|---|")
        for key in sorted(split):
            print("| `%s` | %s |" % (key, ", ".join(split[key])))
    else:
        print("none")
    print()
    print("## directives sitting below the first anchor")
    print()
    inner = [(b, d) for b in bounds for d in b.get("inner", [])]
    if inner:
        print("| line | directive | part it falls in |")
        print("|---|---|---|")
        for bound, directive in inner:
            print("| %d | `:- %s` | `%s` |"
                  % (directive["start"], directive["text"],
                     bound["part"]["file"]))
        print()
        print("Each one moves up into the module head file, above the includes.")
    else:
        print("none")
    print()
    print("## cross-part call edges")
    print()
    print("| from | to | callees |")
    print("|---|---|---|")
    edges = 0
    for bound in bounds:
        outgoing = {}
        for call in sorted(bound["calls"]):
            if call not in in_file or call in bound["defs"]:
                continue
            for other in bounds:
                if other is bound or call not in other["defs"]:
                    continue
                outgoing.setdefault(other["part"]["file"], []).append(call)
        for target in sorted(outgoing):
            edges += 1
            names = outgoing[target]
            shown = ", ".join("`%s`" % n for n in names[:8])
            if len(names) > 8:
                shown += ", +%d more" % (len(names) - 8)
            print("| `%s` | `%s` | %s |"
                  % (bound["part"]["file"], target, shown))
    print()
    print("%d directed part pairs" % edges)
    print()
    print("## what each part owns")
    print()
    print("| part | owns |")
    print("|---|---|")
    for bound in bounds:
        print("| `%s` | %s |" % (bound["part"]["file"], bound["part"]["owns"]))


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    mode, path = sys.argv[1], sys.argv[2]
    if mode == "outline":
        outline(load(path))
    elif mode == "report":
        report(load(path))
    else:
        raise SystemExit(__doc__)


main()
