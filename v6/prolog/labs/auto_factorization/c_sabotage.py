"""Sabotage receipt for the dl6 grade: a grade that cannot go red proves nothing.

Two independent sabotages, each expected to flip a DIFFERENT column, so a grade
passing both by accident is not possible:

  hop_floor   `prior < 64` becomes `prior < 2`, truncating the closure. Depth
              and therefore the derived prefix must both move.
  folder_swap one file is fed the wrong folder. Depth must NOT move (it does not
              read folders) while cohesion must.

MEASURED, and it corrects the draft expectation this file was written with: the
folder swap moves cohesion and leaves the PREFIX alone. The prefix is a dense
rank over the distinct depths a folder holds, so relocating one file changes no
prefix whenever its depth is already represented in both folders' ladders. That
is a real coarseness in the metric, recorded rather than sabotaged around.
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


def run(program, schedule, log, budget):
    goal = f"oracle('{program}','{schedule}')"
    proc = subprocess.run(
        [str(LAB / "cap.sh"), str(budget), "sabotage oracle", "swipl", "-q", "-l",
         str(REPO / "v6/prolog/compile/scripts/dl6_oracle.pl"), "-g", goal, "-g", "halt"],
        capture_output=True, text=True, cwd=str(REPO / "v6/prolog/compile/scripts"))
    Path(log).write_text(proc.stdout)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr[-2000:])
    return proc.returncode


def state(log):
    rows = defaultdict(set)
    for line in Path(log).read_text().strip().split("\n"):
        if line.startswith("{"):
            for rel, delta in json.loads(line)["deltas"].items():
                for row in delta.get("add", []):
                    rows[rel].add(tuple(row))
    return rows


def compare(rows):
    referee_depth = {p: int(d) for p, d in read_tsv("depth.tsv")}
    referee_prefix = {r["path"]: r["derived"]
                      for r in json.loads((OUT / "rename_table.json").read_text())["rows"]}
    per_folder = json.loads((OUT / "referee.json").read_text())["cohesion"]["folder"]["per_group"]
    depth = {p: int(d) for p, d in rows.get("file_depth", ())}
    prefix = {p: int(n) for p, n in rows.get("file_prefix", ())}
    internal = {f: int(n) for f, n in rows.get("internal_total", ())}
    crossing = {f: int(n) for f, n in rows.get("crossing_total", ())}
    return {
        "depth_mismatch": sum(1 for p in referee_depth if referee_depth[p] != depth.get(p)),
        "prefix_mismatch": sum(1 for p in referee_prefix if referee_prefix[p] != prefix.get(p)),
        "cohesion_mismatch": sum(
            1 for f, d in per_folder.items()
            if d["internal"] != internal.get(f, 0) or d["cross"] != crossing.get(f, 0)),
    }


def main():
    results = {}

    sabotaged = OUT / "sabotage_hop_floor.dl6"
    sabotaged.write_text((LAB / "factorize.dl6").read_text().replace("prior < 64", "prior < 2"))
    if run(sabotaged, OUT / "factorize.schedule.json", OUT / "sabotage_hop.jsonl", 900) != 0:
        return 1
    results["hop_floor"] = compare(state(OUT / "sabotage_hop.jsonl"))

    schedule = json.loads((OUT / "factorize.schedule.json").read_text())
    swapped = 0
    for arrival in schedule[0]:
        if arrival["rel"] == "file_folder" and arrival["row"][0].startswith("v6/prolog/compile/") and not swapped:
            arrival["row"][1] = "v6/prolog"
            swapped = 1
    path = OUT / "sabotage_folder.schedule.json"
    path.write_text(json.dumps(schedule))
    if run(LAB / "factorize.dl6", path, OUT / "sabotage_folder.jsonl", 900) != 0:
        return 1
    results["folder_swap"] = compare(state(OUT / "sabotage_folder.jsonl"))

    expected = {
        "hop_floor": lambda r: r["depth_mismatch"] > 0 and r["prefix_mismatch"] > 0,
        "folder_swap": lambda r: r["depth_mismatch"] == 0 and r["cohesion_mismatch"] > 0,
    }
    verdict = {name: {"counts": data, "flipped_as_expected": expected[name](data)}
               for name, data in results.items()}
    (OUT / "sabotage.json").write_text(json.dumps(verdict, indent=2, sort_keys=True) + "\n")
    print(json.dumps(verdict, indent=2, sort_keys=True))
    holds = all(v["flipped_as_expected"] for v in verdict.values())
    print("SABOTAGE RECEIPT HOLDS" if holds else "SABOTAGE RECEIPT FAILS")
    return 0 if holds else 1


if __name__ == "__main__":
    sys.exit(main())
