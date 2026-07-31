"""Grade numbering.dl6 against the referee: cohesion on all four axes,
modularity as an exact scaled integer, folder-quotient depth and cycles, and
every rename-table check.
"""
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"
REPO = LAB.parents[3]
AXES = ["file", "folder", "package", "plane"]


def read_tsv(name):
    return [line.split("\t") for line in (OUT / name).read_text().strip().split("\n")]


def hand_prefix(path):
    base = path.rsplit("/", 1)[1]
    digits = ""
    for char in base:
        if char.isdigit():
            digits += char
        else:
            break
    return int(digits) if digits and base[len(digits):].startswith("_") else None


def write_schedule():
    nodes = read_tsv("file_nodes.tsv")
    batch = []
    for index, (path, package, folder, _origin) in enumerate(sorted(nodes)):
        batch.append({"rel": "node_index", "sign": "add", "row": [path, index]})
        batch.append({"rel": "file_group", "sign": "add", "row": ["file", path, path]})
        batch.append({"rel": "file_group", "sign": "add", "row": ["folder", path, folder]})
        batch.append({"rel": "file_group", "sign": "add", "row": ["package", path, package]})
        batch.append({"rel": "file_group", "sign": "add", "row":
                      ["plane", path, "prolog" if path.endswith(".pl") else "typescript"]})
        prefix = hand_prefix(path)
        if prefix is not None:
            batch.append({"rel": "hand_prefix", "sign": "add", "row": [path, prefix]})
    for tail, head, _plane in read_tsv("file_edges.tsv"):
        batch.append({"rel": "dep", "sign": "add", "row": [tail, head]})
    (OUT / "numbering.schedule.json").write_text(json.dumps([batch]) + "\n")
    return len(batch)


def run(budget):
    goal = f"oracle('{LAB / 'numbering.dl6'}','{OUT / 'numbering.schedule.json'}')"
    proc = subprocess.run(
        [str(LAB / "cap.sh"), str(budget), "numbering oracle", "swipl", "-q", "-l",
         str(REPO / "v6/prolog/compile/scripts/dl6_oracle.pl"), "-g", goal, "-g", "halt"],
        capture_output=True, text=True, cwd=str(REPO / "v6/prolog/compile/scripts"))
    (OUT / "numbering.ticklog.jsonl").write_text(proc.stdout)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr[-4000:])
    return proc.returncode


def state():
    rows = defaultdict(set)
    for line in (OUT / "numbering.ticklog.jsonl").read_text().strip().split("\n"):
        if line.startswith("{"):
            for rel, delta in json.loads(line)["deltas"].items():
                for row in delta.get("add", []):
                    rows[rel].add(tuple(row))
    return rows


def main():
    arrivals = write_schedule()
    if run(2400) != 0:
        print("NUMBERING ORACLE FAILED")
        return 1
    rows = state()
    referee = json.loads((OUT / "referee.json").read_text())
    problems = []

    engine_internal = {(a, g): int(n) for a, g, n in rows.get("axis_internal_total", ())}
    engine_crossing = {(a, g): int(n) for a, g, n in rows.get("axis_crossing_total", ())}
    for axis in AXES:
        for group, data in referee["cohesion"][axis]["per_group"].items():
            if data["internal"] != engine_internal.get((axis, group), 0):
                problems.append(("internal", axis, group, data["internal"],
                                 engine_internal.get((axis, group), 0)))
            if data["cross"] != engine_crossing.get((axis, group), 0):
                problems.append(("cross", axis, group, data["cross"],
                                 engine_crossing.get((axis, group), 0)))

    edges = int(next(iter(rows["und_total"]))[0])
    scaled = {a: int(v) for a, v in rows.get("modularity_scaled", ())}
    modularity = {}
    for axis in AXES:
        engine_q = scaled.get(axis, 0) / (4 * edges * edges)
        referee_q = referee["cohesion"][axis]["modularity"]
        modularity[axis] = {"engine_scaled": scaled.get(axis), "engine_q": round(engine_q, 4),
                            "referee_q": referee_q, "agrees": round(engine_q, 4) == referee_q}
        if not modularity[axis]["agrees"]:
            problems.append(("modularity", axis, referee_q, round(engine_q, 4)))

    engine_prefix = {p: int(n) for p, n in rows.get("file_prefix", ())}
    referee_prefix = {r["path"]: r["derived"]
                      for r in json.loads((OUT / "rename_table.json").read_text())["rows"]}
    for path, value in referee_prefix.items():
        if engine_prefix.get(path) != value:
            problems.append(("prefix", path, value, engine_prefix.get(path)))

    verdict = {
        "arrivals": arrivals,
        "und_edges": edges,
        "modularity": modularity,
        "cohesion_problems": [p for p in problems if p[0] in ("internal", "cross")],
        "hand_violations": sorted(rows.get("hand_violation", ())),
        "derived_violations": sorted(rows.get("derived_violation", ())),
        "cycle_reach_ins": sorted({(t, h) for t, h, _tf, _hf in rows.get("cycle_reach_in", ())}),
        "folder_cycle": sorted(f for (f,) in rows.get("folder_cycle", ())),
        "folder_depth": sorted((f, int(d)) for f, d in rows.get("folder_depth", ())),
        "hand_agrees": len(rows.get("hand_agrees", ())),
        "hand_differs": len(rows.get("hand_differs", ())),
        "metric_blind": sorted(rows.get("metric_blind", ())),
        "problems": problems,
    }
    (OUT / "numbering_grade.json").write_text(json.dumps(verdict, indent=2, sort_keys=True) + "\n")
    print(json.dumps(verdict, indent=2, sort_keys=True))
    print("NUMBERING GRADE HOLDS" if not problems else "NUMBERING GRADE FAILS")
    return 0 if not problems else 1


if __name__ == "__main__":
    sys.exit(main())
