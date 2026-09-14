# How the project was steered, v1 to v8

Read after the five version reports in this folder. Written from them, plus the v5 to v8 record this session holds.

## The goal never moved, the engine moved seven times

The README pitch from 2026-03-24 survived every rewrite word for word: thirty repos a week, grep, cd, grep again, squint at YAML. Every version answered the same sentence. v1 batch indexed it. v2 made the op own everything. v3 made ops narrow and complete. v4 fused pure bodies into SQL. v5 shrank the whole thing to relations, rules, and a fixpoint. v6 compiled that to two targets through prolog. v7 made the compiler itself a datalog program. v8 ported it to Rust. A goal concrete enough to fit in a paragraph is what let seven engines die under it without the project dying.

## Two kinds of endings

Each version ended on one of two signals.

| version | ending signal | your words at the time |
|---|---|---|
| v1 | comprehension | "GUESSES what's wrong instead of scanner reporting it", "No guessing" |
| v2 | measurement | the swc bench, 3x, "a loop of N iterations should do O(1) expensive work" |
| v3 | comprehension | "eight months in", "i'm tired of your fucking opinions jackass" |
| v4 | measurement | 2.57x slower incremental than cold, 197 seconds for one tick |
| v5 to v7 | design ceiling | one target became two, then the compiler became the program |

The measured endings were calm. The bench said the shape was wrong, the next commit started the new shape, and the verdict is still readable today in `v2/CLAUDE.md` and `v4/perf/baseline.toml`. The comprehension endings were angry, and their reasons live in chat logs that took a lane an hour to find. The lesson written into the repo laws afterward is the same one both times: measure before asserting, three runs, never from the whole gate.

## Rewrites landed beside, never in place

`v2/` next to `crates/`, `v3/` next to `v2/`, v4 after a clearing session that archived everything, v5 as one commit. No version was ever migrated. The cost is that old code died without a funeral and a lane had to dig it back up. The benefit is that no version paid a migration tax, and ideas crossed even when code did not: `$NAME` as capture and column, one table per rule, SQLite as the only store with SQL as the only query surface, ast-grep as the matcher, numbered file prefixes, the LSP.

## The editor outran the engine every time

The LSP was in v1's last fifteen commits, v2's first day, v3's `server` crate at 5,265 lines, v4's "unlock lsp workflows", and v6's open list. It was the pressure that broke v1 and the sprawl that ended v3. Each time the editor was allowed inside the engine it ate the session. The rule that came out of it, the LSP displays what the scanner reports and does no analysis, was written on 2026-04-12 and is still the right rule.

## Each paradigm got its own version

RxJS framing arrived as a note in v1 and became v2's cursor. The effect runtime was v3. Differential dataflow was v4, gated on "we don't know till we measure", and parked. Datalog was v5. Prolog as compiler was v6. Compiler as datalog program was v7. Rust was v8. The one that held was the smallest: v5's model of relations, rules, stratified negation, and recursion to a fixpoint is the model dl8 evaluates today, and it landed as about 1,430 lines. Small and measured survived. Big and clever got a version and a funeral.

## The laws are the compressed endings

Every standing law in `CLAUDE.md` is one of these endings folded up. The comment budget is v3's sprawl. The subscribe law is v2's tokio shape. The 10-second law is v4's 197 seconds. "Doubt yourself before asserting" is v1's guessing analyzer. The sparring protocol, one topic per session, is the 2026-04-29 clearing. Steering agents turned out to be the same job as steering yourself, and the laws are where the steering got cheap.

## What to keep doing, what to change

Keep: the one-paragraph goal, rewrites beside rather than in place, benches before verdicts, and laws written the day the pain happens.

Change: write the ending note the day a version ends, in one file, with the number or the sentence that ended it. v2 and v4 had that and are clear today. v1 and v3 had it scattered across chat logs and cost a lane to reassemble. And keep the editor out of the engine on purpose rather than by exhaustion.
