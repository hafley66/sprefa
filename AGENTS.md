# Agent Instructions

## Agent lanes (boop)

Run `boop --help` before spawning or messaging any agent lane. The help text
is the doctrine: lane create flow, completion hail, the two liveness checks,
and ack semantics. Discover boop by using boop.

## Implementation delegation and review

Use Sol (`gpt-5.6-sol`) subagents for code writing. The primary agent owns
coordination, integration, fit and finish, and sampled review of substantive
changes and their tests. Review the changed behavior and integration seams;
avoid duplicating implementation work that already satisfies the task.

## Kernel changes require user participation

Before changing compiler kernel behavior or semantics, explain the current
behavior, the proposed change, and its consequences using concrete DL7 examples.
Obtain explicit user approval before implementing the change. Broad requests
to keep going or make progress do not authorize kernel changes. This requirement
also applies to delegated work; subagents must surface proposed kernel changes
before implementing them.

Keep the user involved in decisions about the graph core, type semantics,
binding, rules, evaluation, and macrotime/comptime phase boundaries. If a task
requires changing these contracts, pause that portion for explanation and
approval while continuing independent, authorized work.

## Compiler reading support

Keep explanations concise while covering the relevant details. For longer
explanations, deliver one coherent page at a time and stop for the user to
read and catch up before continuing. Avoid sending all pages in one turn.

Assume the user needs an explanation without having read the implementation.
Walk through relevant signatures, inputs and outputs, concrete source examples,
and the sequence of transformations. Explain unfamiliar terms when introduced.
Prefer TypeScript-shaped pseudocode for signatures, examples, and walkthroughs
where possible, especially RxJS-shaped pseudocode for wiring, asynchronous
flows, coroutines, actors, and server logic. Explain these through async scans,
actor state and message flow, locks, semaphores, and network I/O where applicable.
Make concurrency, sequencing, and state updates explicit in those examples.
This is an explanatory notation preference, not a requirement to implement
the system using RxJS.
When modeling Rust, use Rust-shaped notation to preserve its semantics,
including the TSI complexity being modeled. Use Rust traits, enums, and
pattern matching where needed to explain the relevant contracts and cases.
Include Prolog explanations when discussing Prolog implementation or semantics;
the user is learning Prolog by directing AI-written Prolog that translates
Lisp-shaped DL7 into Prolog, with Datalog-style checkability and compile-time
clock checking as goals.
Explain the connections between source forms, compiler Prolog, lowered rules,
and the checks actually enforced. Introduce syntax and concepts as encountered, and
walk through concrete queries, variable bindings, unification, and backtracking
where relevant. Pair these with pseudo-TypeScript when it helps understanding.
The user wants to learn algorithms expressed through Prolog and Horn clauses.
Explain clause heads and bodies, recursive rules, and how a concrete query
proceeds. Connect Prolog to SQL and relational algebra through concrete examples
of joins, selection, projection, and recursive queries. Explain relevant
differences in evaluation, duplicates, ordering, and negation when they arise.
Use modes as a teaching anchor: output arguments in trailing positions and
multiple outputs became clearer to the user through modes. When explaining a
predicate, show its intended call modes and an example of arguments before and
after success. Distinguish multiple output bindings in one solution from multiple
solutions through backtracking. Explain which modes the implementation supports;
argument position alone does not determine input/output behavior.
Include Lisp explanations when discussing forms, evaluation, quoting,
quasiquoting, and macros. Introduce these concepts through concrete examples
and show the relevant expansion or evaluation steps. Explain which semantics
DL7 actually implements when drawing comparisons with other Lisps.

V7's design intent is a relational compiler and graph type system with a
Lisp-shaped foundation and a minimal set of kernel primitives. Explain how
language features compose from those primitives. TypeSpec is a reference for
the type-system/compiler role; its model and feature parity are outside the
acceptance criteria. Distinguish implemented type and clock checks from planned
ones when explaining this design.

The compiler, sprefa-extract, boop, and soopy distill the user's accumulated
language-learning and implementation lessons: "how to do X, preferably in
language Y." Support that learning through explanations of the language
mechanisms used in the actual code and useful cross-language comparisons.
Preserve the user's participation in those choices as implementation progresses.

Provide absolute file links in dependency/reading order for follow-up reading.
Keep proposed behavior, implemented behavior, and tested behavior distinguishable.

## D2 explanations

Prefer one shared graph with stable node/edge IDs, grouped inputs, and colored,
styled parallel edges for algorithm differences. Explain inline or in grouped
note boxes pointing to nodes/edges; avoid large trailing tables/legends. Use
tooltips for secondary detail. Include concrete before/after values and short code.
Save as `.d2`, avoid reserved identifiers, and validate with D2.

## V6 Status

V6 planning is underway — start at `v6/README.md`, plans in `v6/plans/`. V5
remains the shipping version. V6 exists because the same problems (storage
seam, daemon wire, build-vs-buy, repo/rev identity) kept being re-solved
inside V5's single crate; the V6 arc extracts crate and trait boundaries
instead of rewriting. Current phase: plans only, then hollow traits reviewed
by a human — no behavioral rewrites, no call-site migrations yet.

## Non-Interactive Shell Commands

**ALWAYS use non-interactive flags** with file operations to avoid hanging on confirmation prompts.

Shell commands like `cp`, `mv`, and `rm` may be aliased to include `-i` (interactive) mode on some systems, causing the agent to hang indefinitely waiting for y/n input.

**Use these forms instead:**
```bash
# Force overwrite without prompting
cp -f source dest           # NOT: cp source dest
mv -f source dest           # NOT: mv source dest
rm -f file                  # NOT: rm file

# For recursive operations
rm -rf directory            # NOT: rm -r directory
cp -rf source dest          # NOT: cp -r directory
```

**Other commands that may prompt:**
- `scp` - use `-o BatchMode=yes` for non-interactive
- `ssh` - use `-o BatchMode=yes` to fail instead of prompting
- `apt-get` - use `-y` flag
- `brew` - use `HOMEBREW_NO_AUTO_UPDATE=1` env var

## Rust Formatting

Formatting churn is not a review concern. Do not spend implementation or review
time minimizing, reconstructing, or undoing formatter-only changes.

Do not pass individual file paths to `cargo fmt`; Cargo may format every target
in the workspace anyway. Do not run the repository-wide formatter during
implementation. Run `cargo fmt` once immediately before commit and include all
resulting formatting in that commit.

## CI Reporting

CI means build, compile, and test execution. Report whether new work adds,
changes, or removes CI coverage. Do not report formatter or linter status.
Report only current results; omit stale, previous, and baseline-matching data.

## Issues (issuectl)

`issues/` edits made by `issuectl` commit directly on `main`. No branch, no
PR, no lane for an issue change. Push after committing.
