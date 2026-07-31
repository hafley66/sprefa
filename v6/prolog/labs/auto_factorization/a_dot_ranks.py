"""Q1 receipt: graphviz assigns ranks by longest path from the sources, so on
the REVERSED dependency graph dot's rank is exactly the height this lab calls
depth. If the two ever disagree, one of them has the graph wrong.
"""
import json
import subprocess
from pathlib import Path

LAB = Path(__file__).resolve().parent
OUT = LAB / "out"


def read_tsv(name):
    return [line.split("\t") for line in (OUT / name).read_text().strip().split("\n")]


def main():
    nodes = [r[0] for r in read_tsv("file_nodes.tsv")]
    edges = [(t, h) for t, h, _ in read_tsv("file_edges.tsv")]
    lines = ["digraph reversed_dependencies {", "  rankdir=TB;"]
    lines += [f'  "{n}";' for n in sorted(nodes)]
    lines += [f'  "{h}" -> "{t}";' for t, h in sorted(edges)]
    lines.append("}")
    dot_path = OUT / "reversed.dot"
    dot_path.write_text("\n".join(lines) + "\n")

    plain = subprocess.run([str(LAB / "cap.sh"), "60", "dot plain", "dot", "-Tplain", str(dot_path)],
                           capture_output=True, text=True)
    if plain.returncode != 0:
        print(f"GRAPHVIZ UNAVAILABLE exit {plain.returncode}")
        return
    ys = {}
    for line in plain.stdout.split("\n"):
        parts = line.split()
        if parts and parts[0] == "node":
            ys[parts[1].strip('"')] = float(parts[3])
    # dot's y grows upward and its sources sit at the top, so rank 0 is the
    # LARGEST y. Sorting ascending would report the ladder upside down.
    tiers = {y: i for i, y in enumerate(sorted(set(ys.values()), reverse=True))}
    dot_rank = {node: tiers[y] for node, y in ys.items()}
    referee = {p: int(d) for p, d in read_tsv("depth.tsv")}
    mismatch = sorted((n, referee.get(n), dot_rank.get(n))
                      for n in set(referee) | set(dot_rank)
                      if referee.get(n) != dot_rank.get(n))
    # The property BOTH orderings must have: every dependency edge runs strictly
    # downhill. Equality of the two numbers is a different and stronger claim,
    # and it is false by construction -- dot ranks with network simplex, which
    # MINIMISES total edge length, while height MAXIMISES distance to a leaf.
    depth_uphill = [(t, h) for t, h in edges if referee[t] <= referee[h]]
    dot_uphill = [(t, h) for t, h in edges if dot_rank[t] <= dot_rank[h]]
    pulled = [(n, referee[n], dot_rank[n]) for n, _r, _d in mismatch if dot_rank[n] > referee[n]]

    verdict = {"nodes_ranked": len(dot_rank), "distinct_dot_ranks": len(tiers),
               "max_depth": max(referee.values()), "max_dot_rank": max(dot_rank.values()),
               "edges": len(edges),
               "depth_uphill_edges": depth_uphill, "dot_uphill_edges": dot_uphill,
               "equal_rank_nodes": len(dot_rank) - len(mismatch),
               "mismatch_count": len(mismatch),
               "mismatch_all_pulled_down": len(pulled) == len(mismatch),
               "mismatch": mismatch}
    (OUT / "dot_ranks.json").write_text(json.dumps(verdict, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: v for k, v in verdict.items() if k != "mismatch"}, indent=2, sort_keys=True))
    holds = not depth_uphill and not dot_uphill and verdict["mismatch_all_pulled_down"]
    print("DOT RANK RECEIPT HOLDS" if holds else "DOT RANK RECEIPT FAILS")


if __name__ == "__main__":
    main()
