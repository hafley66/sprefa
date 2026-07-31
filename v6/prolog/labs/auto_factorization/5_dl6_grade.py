"""Write the schedule the dl6 program is fed, run the reference engine over it,
and diff every derived rel against the networkx referee row for row.
"""
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"
REPO = LAB.parents[3]


def read_tsv(name):
    return [line.split("\t") for line in (OUT / name).read_text().strip().split("\n")]


def write_schedule():
    batch = []
    for path, _package, folder, _origin in read_tsv("file_nodes.tsv"):
        batch.append({"rel": "file_folder", "sign": "add", "row": [path, folder]})
    for tail, head, _plane in read_tsv("file_edges.tsv"):
        batch.append({"rel": "dep", "sign": "add", "row": [tail, head]})
    (OUT / "factorize.schedule.json").write_text(json.dumps([batch]) + "\n")
    return len(batch)


def run_oracle(budget):
    script = LAB / "cap.sh"
    goal = f"oracle('{LAB / 'factorize.dl6'}','{OUT / 'factorize.schedule.json'}')"
    proc = subprocess.run(
        [str(script), str(budget), "dl6 oracle", "swipl", "-q", "-l",
         str(REPO / "v6/prolog/compile/scripts/dl6_oracle.pl"), "-g", goal, "-g", "halt"],
        capture_output=True, text=True, cwd=str(REPO / "v6/prolog/compile/scripts"))
    (OUT / "factorize.ticklog.jsonl").write_text(proc.stdout)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr[-4000:])
    return proc.returncode


def final_state():
    rows = defaultdict(set)
    for line in (OUT / "factorize.ticklog.jsonl").read_text().strip().split("\n"):
        if not line.startswith("{"):
            continue
        for rel, delta in json.loads(line)["deltas"].items():
            for row in delta.get("add", []):
                rows[rel].add(tuple(row))
            for row in delta.get("del", []):
                rows[rel].discard(tuple(row))
    return rows


def main():
    arrivals = write_schedule()
    code = run_oracle(900)
    if code != 0:
        print(f"ORACLE EXIT {code}")
        return code
    state = final_state()

    referee_depth = {p: int(d) for p, d in read_tsv("depth.tsv")}
    engine_depth = {p: int(d) for p, d in state.get("file_depth", ())}
    depth_mismatch = sorted(
        (p, referee_depth.get(p), engine_depth.get(p))
        for p in set(referee_depth) | set(engine_depth)
        if referee_depth.get(p) != engine_depth.get(p))

    referee = json.loads((OUT / "referee.json").read_text())
    per_folder = referee["cohesion"]["folder"]["per_group"]
    engine_internal = {f: int(n) for f, n in state.get("internal_total", ())}
    engine_crossing = {f: int(n) for f, n in state.get("crossing_total", ())}
    cohesion_mismatch = []
    for folder, data in per_folder.items():
        if data["internal"] != engine_internal.get(folder, 0):
            cohesion_mismatch.append(("internal", folder, data["internal"], engine_internal.get(folder, 0)))
        if data["cross"] != engine_crossing.get(folder, 0):
            cohesion_mismatch.append(("cross", folder, data["cross"], engine_crossing.get(folder, 0)))

    rename = json.loads((OUT / "rename_table.json").read_text())
    referee_prefix = {r["path"]: r["derived"] for r in rename["rows"]}
    engine_prefix = {p: int(n) for p, n in state.get("file_prefix", ())}
    prefix_mismatch = sorted(
        (p, referee_prefix.get(p), engine_prefix.get(p))
        for p in set(referee_prefix) | set(engine_prefix)
        if referee_prefix.get(p) != engine_prefix.get(p))

    verdict = {
        "arrivals": arrivals,
        "rels_derived": sorted(state),
        "reach_rows": len(state.get("reach", ())),
        "cycle_files": sorted(state.get("cycle_file", ())),
        "depth_rows": len(engine_depth),
        "depth_mismatch": depth_mismatch,
        "cohesion_mismatch": cohesion_mismatch,
        "prefix_rows": len(engine_prefix),
        "prefix_mismatch": prefix_mismatch,
    }
    (OUT / "dl6_grade.json").write_text(json.dumps(verdict, indent=2, sort_keys=True) + "\n")
    print(json.dumps(verdict, indent=2, sort_keys=True))
    holds = not depth_mismatch and not cohesion_mismatch and not prefix_mismatch
    print("DL6 GRADE HOLDS" if holds else "DL6 GRADE FAILS")
    return 0 if holds else 1


if __name__ == "__main__":
    sys.exit(main())
