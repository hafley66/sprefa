# Lane: Flix recon lab: first-class datalog, schema rows, traits, speed

## Base
`git merge --ff-only 3f9d2cd8b70f378ab4a9c132e43006068186bfbe` is your FIRST
action. Failure = STOP AND REPORT.
Repo: `~/projects/sprefa-v6`. Worktree: `.boop-worktrees/feature/flix-recon-lab`.
You create `docs/labs/flixlab/` — a sibling of the existing
`docs/labs/mercurypl/` (read its `LAB.md` + `FINDINGS.md` ONLY as a format
example; do not modify anything there).

## Why this exists
dl6 (sprefa v6) is a datalog-over-code language. Flix is the one production
language where datalog programs are first-class typed values (schema rows,
`#{...}` constraint sets, `query`/`inject`/`solve`). Chris wants the same
recon that was done for Mercury: verbatim compiler receipts on how Flix
types, composes, and executes datalog, plus a speed point. Findings only;
design calls are Chris's.

## Toolchain setup (do this exactly; STOP AND REPORT on failure)
```bash
java -version              # need 21+; if missing or older: brew install openjdk@21 and use
                           # /opt/homebrew/opt/openjdk@21/bin/java everywhere below
mkdir -p ~/projects/sprefa-v6/.boop-worktrees/feature/flix-recon-lab/docs/labs/flixlab/toolchain
cd docs/labs/flixlab/toolchain
curl -L -o flix.jar https://github.com/flix/flix/releases/latest/download/flix.jar
java -jar flix.jar --version   # record in LAB.md
```
Per probe: a project dir made with `java -jar ../../toolchain/flix.jar init`,
source in `src/Main.flix`, run with `java -jar ../../toolchain/flix.jar run`.
Record exact commands in `cmd.txt`, full output in `output.txt`, exit code in
`rc.txt`. Do NOT commit flix.jar: add `docs/labs/flixlab/toolchain/flix.jar`
to a `.gitignore` INSIDE `docs/labs/flixlab/`.

## Deliverables
`docs/labs/flixlab/LAB.md` (goal, non-goals "no adoption, no design
decisions", toolchain versions), `probes/NN_<name>/`, `FINDINGS.md` table
(`probe | Flix construct | key output line (verbatim) | note`), `SYNTHESIS.md`
(tables: what Flix types about datalog, composition mechanics, speed, and a
dl6-overlap table).

## Probes

### f01_hello
`init` template, `run`. Receipt: toolchain works, version recorded.

### f02_datalog_values
Datalog as a typed value. Start from this shape and fix per compiler errors:
```flix
def main(): Unit \ IO =
    let edges = #{ Edge(1, 2). Edge(2, 3). Edge(3, 4). };
    let rules = #{
        Path(x, y) :- Edge(x, y).
        Path(x, y) :- Path(x, z), Edge(z, y).
    };
    let result = query edges, rules select (x, y) from Path(x, y);
    println(result)
```
Receipt: constraint sets are values with types; record the inferred type if
the compiler prints one (try introducing a deliberate type error to make it
print the schema row type, save that error verbatim as part of the receipt).

### f03_schemarow
A function that takes an open constraint set and adds rules:
`def withClosure(db: #{ Edge(Int32, Int32) | r }): #{ Edge(Int32, Int32), Path(Int32, Int32) | r }`
(fix the exact spelling per compiler messages). Call it with two different
databases that carry extra predicates. Receipt: open schema composition, the
row variable `r` in the printed type/signature.

### f04_traits
One trait, one instance, a call through the trait; then a second overlapping
instance to capture the coherence error verbatim. Mirrors mercury probes
08/14 for cross-comparison.

### f05_tc_speed
Transitive closure on a 1000-node cycle via flix datalog:
generate `Edge(i, (i+1) mod 1000)` facts in code (List.range + inject or a
`#{}` built by folding), solve `Path`, count tuples (must print 1000000).
3 timed runs (`/usr/bin/time -p java -jar ... run`). ALSO record JVM startup
separately: time `f01_hello`'s run 3x, so the datalog cost can be separated
from JVM+flix-compile cost. Comparison numbers already measured on this
machine, put them in your FINDINGS row: mercury semi-naive 0.84-0.99s,
swipl same algorithm 41.9s (`docs/labs/mercurypl/probes/19_tc/`).

### f06_aggregates_lattice
Two receipts: (a) an aggregate in a query (count or sum of Path pairs via
`query ... select` with a fold, or Vector.length on the result); (b) the
lattice-semantics feature if it is still in current Flix (`Butnot`/lattice
annotations): a shortest-path over a lattice example from the flix docs,
adapted; if lattices were removed or gated, record that finding with the
doc/changelog URL.

### f07_dl6_overlap
No code. A table mapping dl6 constructs to Flix datalog constructs, one row
each, marking exact / approximate / absent:
rel declaration with typed columns; keyed (primary key); derived rules;
negation; aggregates; option columns; enum variants; incremental update /
retraction; compile-to-storage (SQLite). For "incremental/retraction" find
and cite whether Flix re-solves from scratch per `query` (search the flix
docs/paper; cite URL). Sources: https://doc.flix.dev/fixpoints.html and
linked pages; cite every row.

## Ownership
You own ONLY `docs/labs/flixlab/**` in your sprefa-v6 worktree. Forbidden:
`docs/labs/mercurypl/**` (a concurrent lane owns it), every other sprefa-v6
path, all writes under `~/projects/sprefa` and `~/projects/hafley-rs`.

## Validation before you finish
```bash
ls docs/labs/flixlab/probes/*/output.txt | wc -l   # >= 6
grep -c '^|' docs/labs/flixlab/FINDINGS.md          # >= probe count + 2
git status --short                                   # no flix.jar staged
```
Commit on your branch. A lane that exits without committing delivered nothing.

## Style laws (non-negotiable)
- No em dashes. Banned words, prose AND identifiers: provenance, substrate,
  load-bearing, regime. "refusal" banned: write "not built yet".
- Tables over prose. Every behavioral claim carries its probe path or URL.
- Iterate each probe until it expresses its INTENT; a probe failing on your
  own syntax typo is not a receipt.
