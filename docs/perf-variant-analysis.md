# Perf variant analysis: overfitting a rule to a bug

This is a design-vibe note, not a spec. It records how the loop-invariant-call
rail got built, what the AI was actually used for, and where this sits relative
to CodeQL / Semgrep / Glean. The motivating question: *can we iteratively
overfit a query to each newly-found perf bug so the next instance of the same
shape gets caught automatically?*

## The bug that started it

`typecheck.rs:439` called `prog_rels(prog)` once per rule inside the typecheck
loop. O(F*R) where R was the rule count; wall time on a K=1000 closure workload
was ~10s, almost all of it before the `[tick]` phase timer could see it. A
flamegraph found it in seconds. The fix was one line (hoist the call above the
loop). The interesting question was the one after the fix: **can the tool we
already have express "find the next one of these."**

## How the AI was used

The AI was not used to *find* the bug. A flamegraph did that, faster and with
zero false positives, which no static rule will match. The AI was used to
**overfit a rule to the bug's shape**, then tighten the rule until it
discriminated bug-from-no-bug. The loop was mechanical and that is the point:

1. **See the shape.** `prog_rels(prog)` is a call inside a loop whose only
   argument is loop-invariant. State that in English; the AI turns it into a
   join over the existing relations.
2. **Find the gap.** Run the rule on the host. It fires 415 times. That is not
   signal. The AI's job is to read the 415, name the noise source, and propose
   the one filter that removes the most of it.
3. **Cut.** Repeat until the rule fires on the bug and is silent on the fixed
   code.

Concretely, four cuts, each validated on the host before the next:

| cut | what it removed | host suspects |
|-----|-----------------|---------------|
| close loop/branch/closure holes (T1) | "the lift can't see into the loop body at all" | — |
| `loop_over` + param-proxy flag (T2) | "no notion of a loop" | 415 |
| `allocates(callee)` cost filter + name resolution (T3) | "f(param) in a loop where f is cheap" | 123 |
| strict antijoin: no loop-carried input (T3) | "the result depends on the iteration" | 13 |
| bind tuple/match patterns | "loop var destructured, looks invariant" | 13 |

After the strict antijoin, the rule fires **exactly once** on a synthetic
prog_rels fixture and **zero** times on the hoisted version. That 1-vs-0 is the
definition of "the rule captures this bug." The 13 residuals on the real host
are all traceable to named precision gaps (field access, conditional calls,
header-vs-body span); none is a real bug.

The meta-procedure, and the thing worth keeping, is steps 1-3 as a loop you run
*every time you fix a bug*. Fix bug -> write the query that would have caught
it -> add it to the rail set. The query does not need to be precise on the first
try; it needs to be *shaped* so the next tightening pass can target the noise.

## Is this what CodeQL / Semgrep / Glean are for?

Short answer: yes for CodeQL and Semgrep, partially for Glean. sprefa is
architecturally a peer of all three; this session built one query of the kind
their ecosystems ship by the thousand.

**CodeQL** is the closest analogue. It is a Datalog dialect over a code database
of extracted facts (tables for calls, defs, exprs, types), queried with
declarative rules. The entire GitHub variant-analysis workflow is *exactly*
steps 1-3 above: a CVE lands, a security engineer writes the query that finds
it, the query gets added to a pack, every repo gets scanned. The loop-invariant
rail here is a perf-bug query in the same shape. The difference is CodeQL's
database is far richer (years of language-team extractors) and its dataflow /
taint libraries are built-in; sprefa's `df_node`/`df_edge` lift is the
hand-rolled v0 of that library.

**Semgrep** is the pattern-shaped subset. You write a pattern with metavariables
(`for $X in $COL { ... $F($ARG) ... }`) and it matches syntactically. sprefa's
`sg` / `ast_yaml` ops are the Semgrep-equivalent layer. Semgrep is faster to
write a rule in and weaker on flow: it has no transitive reachability unless you
opt into taint mode, and even then it's shallower than a Datalog closure. The
reason the prog_rels rule needed `df_edge` + an antijoin (not just an sg
pattern) is that "the argument's reaching def is outside the loop" is a
two-hop dataflow fact, which Semgrep patterns cannot express without the taint
engine. So: Semgrep for syntactic shapes, sprefa/CodeQL when the bug is a flow
fact.

**Glean** is a different layer. It is Facebook's fact store for code (a
Bunch/Datalog-derived database), used as the backing index for code
intelligence at scale: go-to-definition, find-references, refactors, holdings.
Glean *can* express bug queries via derived predicates (you write "derivations"
that are Datalog rules over base facts), and Hack/Infer use it as a foundation.
But Glean's pitch is "one fact store every tool reads from," not "a query pack
for finding bugs." Think of it as the relations-and-refresh layer sprefa's
engine already is, without the curated query library. If sprefa grew a "query
pack" directory of `.dl` files, one per fixed bug, that directory would be the
thing Glean does not ship and CodeQL does.

## Where this is going

The loop-invariant rail is one query. The goal is a *practice*: every perf bug
worth a flamegraph is worth a query added to `std/`. The discipline is the same
one CodeQL query packs enforce:

- the query fires on the bug (1-vs-N test committed);
- the query is silent on the fixed code (0-on-fix test committed);
- the query lives next to the codebase, not in someone's head.

The honest limit, from this session: the rule caught the shape, but the host
had no second instance of it. A rail that is silent on a clean tree is only
worth keeping if the tree will grow. sprefa's will. The cost of the query is
low (a `.dl` file + two tests); the cost of the *next* prog_rels is another
flamegraph and another fix. Asymmetric in favor of writing the query.

The remaining precision gaps (field access, conditional calls, header-vs-body
spans) are fidelity polish on this one query, not new capability. The next
leverage is a *different* bug shape and a new query for it, not perfecting this
one.
