# G6 result

## Retained

- Replaced the three-pass counting/scope/live path with two indexed frontier passes: affected-cone removal, then rederivation from surviving external support.
- Fused frontier, cone, and weight mutations per round. PK `INSERT OR IGNORE` performs deduplication without `SELECT DISTINCT` temp B-trees.
- Public `RelStore` signatures are unchanged.

## Firsthand reads

- Baseline report: SCC 2890.0 ms / 56 statements; DRed-loop 2361.0 ms / 75 statements on CYC 960k.
- Final report: SCC 2097.8 ms / 39 statements; DRed-loop 2259.3 ms / 75 statements. SCC is 0.928x DRed, 7.1% lower wall time. Both return 815240 survivors and match the oracle.
- Final DAG 960k: SCC 1951.8 ms versus counting 435.6 ms. The pure-DAG early-out target did not land.
- Rust peak remains 0.16 MB at CYC 960k while SQLite high-water is 89.04 MB and DB size is 62.15 MB. The flat Rust number is explained by graph/frontier residence in SQLite; SQLite and DB measurements scale from 5.60/3.79 MB at 60k to 89.04/62.15 MB at 960k.

## Rejected

- Simultaneous dead and positive cone frontiers: correct hash, 3248.9 ms / 78 statements. Reverted.
- Counted-positive boundary plus recursive positive-scope CTE: correct hash, 2889.1 ms / 50 statements. Trace assigned 1066.9 ms to scope materialization. Reverted.
- Indexed PK scope frontier: correct hash, 2895.9 ms / 66 statements. Reverted.

## Falsification reruns

- Agreement test was run after the intermediate algorithms and after the retained algorithm: 3/3 each time.
- Pre-report CYC 960k pairs, hash `25ee690520b18777`: SCC 2104.2/2122.4 ms; DRed 2254.9/2249.1 ms.
- Full-report pair: SCC 2097.8 ms; DRed 2259.3 ms, hash identical.
- Post-report pair: SCC 2105.3 ms; DRed 2252.3 ms, hash identical.
- `rg -n 'eprintln' src/` reports only the pre-existing opt-in trace at `src/cascade.rs:69`.
