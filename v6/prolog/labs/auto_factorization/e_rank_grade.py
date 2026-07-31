"""Grade rank.dl6: the ranking step of question 5 has no ORDER BY, so it is a
count of the candidates that beat this one. Checked against python's sort over
the same candidate rows, ties included.
"""
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"
REPO = LAB.parents[3]


def main():
    cuts = json.loads((OUT / "cuts.json").read_text())
    rows = cuts["file_folder_boundaries"] + cuts["symbol_file_boundaries_top"]
    batch = [{"rel": "candidate", "sign": "add",
              "row": [r["cut"], r["edge_delta"], r["depth_after"], r["spliced_edges"]]}
             for r in rows]
    (OUT / "rank.schedule.json").write_text(json.dumps([batch]) + "\n")

    goal = f"oracle('{LAB / 'rank.dl6'}','{OUT / 'rank.schedule.json'}')"
    proc = subprocess.run(
        [str(LAB / "cap.sh"), "600", "rank oracle", "swipl", "-q", "-l",
         str(REPO / "v6/prolog/compile/scripts/dl6_oracle.pl"), "-g", goal, "-g", "halt"],
        capture_output=True, text=True, cwd=str(REPO / "v6/prolog/compile/scripts"))
    (OUT / "rank.ticklog.jsonl").write_text(proc.stdout)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr[-3000:])
        print("RANK ORACLE FAILED")
        return 1

    engine = defaultdict(set)
    for line in proc.stdout.strip().split("\n"):
        if line.startswith("{"):
            for rel, delta in json.loads(line)["deltas"].items():
                for row in delta.get("add", []):
                    engine[rel].add(tuple(row))

    deltas = sorted({r["edge_delta"] for r in rows})
    all_deltas = sorted(r["edge_delta"] for r in rows)
    referee_dense = {r["cut"]: deltas.index(r["edge_delta"]) for r in rows}
    referee_competition = {r["cut"]: all_deltas.index(r["edge_delta"]) for r in rows}
    engine_dense = {c: int(n) for c, n in engine.get("dense_rank", ())}
    engine_competition = {c: int(n) for c, n in engine.get("competition_rank", ())}
    mismatch = sorted(
        [("dense", c, referee_dense[c], engine_dense.get(c))
         for c in referee_dense if referee_dense[c] != engine_dense.get(c)]
        + [("competition", c, referee_competition[c], engine_competition.get(c))
           for c in referee_competition if referee_competition[c] != engine_competition.get(c)])

    best_delta = min(r["edge_delta"] for r in rows)
    referee_best = sorted(r["cut"] for r in rows if r["edge_delta"] == best_delta)
    engine_best = sorted(c for c, _d in engine.get("best_cut", ()))

    verdict = {"candidates": len(rows), "distinct_deltas": len(deltas),
               "rank_mismatch": mismatch, "referee_best": referee_best,
               "engine_best": engine_best, "best_agrees": referee_best == engine_best}
    (OUT / "rank_grade.json").write_text(json.dumps(verdict, indent=2, sort_keys=True) + "\n")
    print(json.dumps(verdict, indent=2, sort_keys=True))
    holds = not mismatch and verdict["best_agrees"]
    print("RANK GRADE HOLDS" if holds else "RANK GRADE FAILS")
    return 0 if holds else 1


if __name__ == "__main__":
    sys.exit(main())
