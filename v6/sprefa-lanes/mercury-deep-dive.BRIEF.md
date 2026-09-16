# Lane: Mercury deep dive: whitelabel feasibility, reflection typegen, parser port

## Base
`git merge --ff-only 3f9d2cd8b70f378ab4a9c132e43006068186bfbe` is your FIRST
action. Failure = STOP AND REPORT.
Repo: `~/projects/sprefa-v6`. Worktree: `.boop-worktrees/feature/mercury-deep-dive`.
The lab you are extending: `docs/labs/mercurypl/` (probes 01-19 already exist;
read `LAB.md`, `FINDINGS.md`, `SYNTHESIS.md` first; your probes start at 20).
Toolchain: `/opt/homebrew/bin/mmc`, Mercury 22.01.8.

## Why this exists
dl6 (sprefa v6) is a datalog whose compiler is SWI-prolog. Chris is weighing
three futures and needs receipts, not opinions:
1. Rewrite the dl6 compiler in Mercury.
2. Whitelabel: the language IS Mercury with addendums; dl6 surface becomes
   Mercury terms/types; typegen becomes reflection over Mercury's type system.
3. Status quo: prolog compiler, Mercury only as one more emitter target.
Design calls stay with Chris. You deliver probes, verbatim compiler output,
and cited findings.

## Deliverables
All inside `docs/labs/mercurypl/` in YOUR worktree. Per probe
`probes/NN_<name>/`: final `.m` sources, `cmd.txt`, `output.txt` (untrimmed),
`rc.txt`. Append one row per probe to `FINDINGS.md` (same table shape).
Append a `## Whitelabel assessment (probes 20+)` section to `SYNTHESIS.md`:
tables and forks only, no recommendations phrased as decisions.

## Probes

### 20_parser_port: the sprefa rel-statement slice as a Mercury char DCG
Port from `~/projects/sprefa/v6/prolog/compile/parse_dl_dcg.pl` lines 460-540
(READ-ONLY): whitespace skipping, `ident`, comma-separated `args`, and the
declaration form `rel name(col: type, col: type).` producing a typed decl
term (define a small du type: `decl ---> rel_decl(string, list(col)); col ---> col(string, string)`).
Receipts: line count of the port vs the prolog original slice, the list of
friction points (each with the compiler error that exposed it), run output on
3 inputs incl. one syntax error showing error position quality.

### 21_reflect_typegen: typegen via RTTI, the whitelabel keystone
Define 2-3 du types shaped like dl6 rel rows (`user(id :: int, name :: string)`
style with field names). Using ONLY stdlib reflection
(`type_desc`, `construct`, `deconstruct`, maybe `univ`), walk the type at
runtime: enumerate constructors, arity, argument types, and FIELD NAMES if
reachable. Emit (a) a TypeScript interface string and (b) a JSON-schema-ish
string from the walk, print both. If field names are NOT reachable through
RTTI, that is a headline finding: name the exact API gap and the fallback
(e.g. `deconstruct.functor_number`, or compile-time codegen instead).
This answers "does the type system help us and we just reflect on that".

### 22_term_to_type: the untyped-to-typed door
`term_conversion.term_to_type` / `type_to_term` round trip on the probe-20
decl type: read a term with `mercury_term_parser.read_term` (probe 17 shows
how), convert to the typed decl, convert back, print. Receipt: a whitelabel
front end = term reader + term_to_type, zero hand grammar. Also record what
happens on a term that does NOT fit the type (the error shape and quality).

### 23_operator_table: the whitelabel surface constraint
Mercury's operator table is fixed (no user-defined operators). Feed the term
reader dl6-shaped spellings and record accept/reject verbatim:
`rel foo(x: int).` — `foo(X) <- bar(X).` — `a := b + 1.` — `x <-> y.` —
`p :- q.` — `#foo(bar).`
One row per spelling in output. This prices which dl6 surface survives as
Mercury terms and which needs its own reader.

### 24_json: the third-party JSON story
Stdlib has no json module (verified). Clone github.com/juliensf/mercury-json
into your probe dir, build it against 22.01.8, round-trip a value of the
probe-21 type. Receipts: does it need a hand instance per type or is it
generic/derived? Build friction verbatim. If the build fights longer than
~20 minutes, record the wall and move on; the finding is the friction itself.

### 25_compiler_shape: how Mercury's own compiler is built (research, no code)
Web sources only, cite URLs: the mercury repo `compiler/` layout — the
parse_tree/HLDS/codegen stage split, roughly how many .m files/lines, and the
fact that the whole compiler is written in Mercury (self-hosted). One table:
stage, module family, role. Purpose: existence proof for "compiler in
Mercury" and a size yardstick for a dl6 port. Secondary: grep the sprefa
prolog compiler (READ-ONLY `~/projects/sprefa/v6/prolog`) and count occurrences
of `=..`, `sub_term(`, and `findall(` as a proxy for untyped-term surgery
that a typed port must turn into du types + explicit traversals; report the
three counts per file in a table (top 10 files).

### 26_queens (only if everything above is done)
n-queens count for N=10 via `solutions/2` in Mercury vs `findall` in SWI
(`swipl` is installed), 3 timed runs each, same algorithm.

## Ownership
You own ONLY `docs/labs/mercurypl/**` in your sprefa-v6 worktree. Forbidden:
`docs/labs/flixlab/**` (a concurrent lane owns it), every other sprefa-v6
path, all writes under `~/projects/sprefa` and `~/projects/hafley-rs`.
Delete build junk (`Mercury/` dirs, binaries, `.mh/.mih`) before committing.

## Validation before you finish
```bash
ls docs/labs/mercurypl/probes/2*/output.txt | wc -l    # >= 6 (20-25)
grep -c '^|' docs/labs/mercurypl/FINDINGS.md            # grew by your probe count
```
Commit on your branch. A lane that exits without committing delivered nothing.

## Style laws (non-negotiable)
- No em dashes. Banned words, prose AND identifiers: provenance, substrate,
  load-bearing, regime. "refusal" banned: write "not built yet".
- Tables over prose. Every behavioral claim carries its probe path or URL.
- Comments in probe sources state only what the code cannot show.
- Findings are forks with receipts; decisions are Chris's.
