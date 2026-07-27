# timeless_rail: the tier-0 exit criterion, labbed

Run: `swipl -q -l v6/prolog/labs/timeless_rail.pl -g go -g halt` (18 PASS, exit 0).

Two v5 lint rails transcribed into the candidate surface and reference-interpreted end to end:

- `.dl/no-new-eprintln.dl` (103 lines): ratchet baseline, waiver range join, negation, count
  aggregation, two diag rules, stage routing, `? diag(...)`.
- `.dl/rails.dl:107-134` (the unwrap budget): count aggregation grouped by path, a changed-file
  join inside the aggregate, `${n}` string interpolation, span columns on the same `diag` rel.

Timeless fragment only. Level rules, stratified negation, count aggregation, facts, `Key(T)` read
as a functional dependency. No `<+`, no `pre`, no `now`, no clock, no tick. Check
`program_is_timeless` proves that mechanically over the program terms.

## VERDICT

**Tier 0 as scoped in `plans/2026-07-27-tier-topology.md` is NOT sufficient for this rail class,
and the shortfall is entirely non-temporal.** Both rails transcribe and run, but they needed
**12 inventions**, of which the topology doc's T0 line ("enum/struct + typed rel cols + Key(Type) as
schema; level rules; stratified negation; aggregation with grouping; facts; snapshot asks") covers
only 6. The other 6 are comparison and arithmetic in bodies, string interpolation, named-column
atoms, column defaults, a singleton rel, and rel union. None of them is hard; all of them are
missing from LANG.md; and the rail is unwritable without any one of them.

Three secondary findings, all with receipts below:

1. **T3 collapses into T0 plus a CLI convention.** The topology doc gives diagnostics their own
   tier. The gate, the severity split, the stage routing, and the `--check` exit code all
   transcribed as ordinary rules over ordinary fact tables, with **zero new syntax** past what the
   rails already needed. `gate_blocked` / `gate_exit` / `check_exit` are eight lines of level rules.
   T3 is a library and a naming convention, not a tier.
2. **`Key(T)` earned nothing on this rail.** The ratchet works identically with an unkeyed
   `eprintln_baseline`. The key buys exactly one thing in tier 0: a static rejection of a
   hand-edited table with two rows for one path (`ratchet_fd_rejects_duplicate_key`). Latest-wins,
   the reason `Key` exists in LANG.md, is a tier-4 payoff with no tier-0 customer here.
3. **Orthogonality claim 2 holds, measured.** The program text is byte-identical between the canned
   world rows this lab feeds and a tier-1 `bind`. Extraction enters as a rel modifier, not as body
   syntax. That is the one claim in the topology doc this lab can confirm rather than assert.

## The transcribed program

```
# .dl/no-new-eprintln.dl + .dl/rails.dl:107-134, candidate surface, tier 0.

enum Severity { Error, Warning }

# ---- extraction: world-fed rels ---------------------------------------- I1
# Tier 1 supplies each of these a bind, e.g.
#   bind eprintln_hit = scan("WORK", "src/**/*.rs") |> match_line(/eprintln!/);
# Tier 0 declares the shape; the test feeds canned rows against the same text.
rel eprintln_hit(path: Path, line_no: Int) from world;
rel waiver_block_comment(path: Path, line_no: Int) from world;
rel waiver_trailing_comment(path: Path, line_no: Int) from world;
rel unwrap_hit(path: Path, line_no: Int, col: Int, end_col: Int) from world;
rel changed_file(path: Path) from world;

rel eprintln_waiver_line(path: Path, waiver_line: Int);
rel eprintln_waived(path: Path, line_no: Int);
rel eprintln_counted(path: Path, line_no: Int);
rel eprintln_count(path: Path, hits: Int);
rel unwrap_count(path: Path, hits: Int);

# ---- the ratchet: in tier 0, Key(T) is a functional dependency --------- I7
rel eprintln_baseline(path: Key(Path), allowed: Int);
eprintln_baseline("src/config.rs", 1);
eprintln_baseline("src/daemon/client.rs", 2);
eprintln_baseline("src/setup/vscode.rs", 1);
eprintln_baseline("src/setup/wire.rs", 1);

# ---- the diag product rel ------------------------------------------ I5, I6
rel diag(path: Path, line_no: Int, severity: Severity, code: Code, msg: Str,
         col: Int = none, end_col: Int = none);
rel diag_stage(code: Code, stage: Stage);
diag_stage("eprintln-exceeded", "agent-turn");
diag_stage("eprintln-exceeded", "commit");
diag_stage("eprintln-new-file", "agent-turn");
diag_stage("eprintln-new-file", "commit");
diag_stage("unwrap-budget", "agent-turn");

# ---- gate policy, as data rather than as language ----------------------
rel severity_rank(severity: Key(Severity), rank: Int);
severity_rank(Warning, 1);
severity_rank(Error, 2);

rel gate_threshold(stage: Key(Stage), min_rank: Int);
gate_threshold("agent-turn", 2);   # rails.dl header: error blocks the agent
gate_threshold("commit", 1);       # this lab's assumption: any diag blocks a commit

rel gate_blocked(stage: Stage);
rel gate_exit(stage: Key(Stage), exit_code: Int);

rel program(name: Key(Str));                                            # I8
program("no-new-eprintln");
rel any_diag(program: Str);
rel check_exit(program: Key(Str), exit_code: Int);

# ---- rules -------------------------------------------------------------

# Two rules, one rel: v5 unions the whole-line `comment` op with the
# trailing-comment `match_line` op.                                       # I12
eprintln_waiver_line(path, waiver_line) <- waiver_block_comment(path, waiver_line);
eprintln_waiver_line(path, waiver_line) <- waiver_trailing_comment(path, waiver_line);

# A waiver on the hit line or the line above exempts the hit.             # I2
eprintln_waived(path, line_no) <-
    eprintln_waiver_line(path, waiver_line),
    eprintln_hit(path, line_no),
    waiver_line >= line_no - 1,
    waiver_line <= line_no;

eprintln_counted(path, line_no) <-                                        # I3
    eprintln_hit(path, line_no),
    !eprintln_waived(path, line_no);

eprintln_count(path, count(line_no)) <- eprintln_counted(path, line_no);  # I4

# A grandfathered file grew past its baseline.                            # I9
diag(
    path: path,
    line_no: 1,
    severity: Warning,
    code: "eprintln-exceeded",
    msg: "${hits} counted eprintln! hits; the grandfathered baseline allows ${allowed}. Convert to tracing, or waive the line with @eprintln-ok: <reason>"
) <-
    eprintln_count(path, hits),
    eprintln_baseline(path, allowed),
    hits > allowed;

# A file with no baseline row at all: flag every hit line precisely.
diag(
    path: path,
    line_no: line_no,
    severity: Warning,
    code: "eprintln-new-file",
    msg: "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>"
) <-
    eprintln_counted(path, line_no),
    !eprintln_baseline(path, _);                                          # I11

# ---- second rail: the unwrap budget ------------------------------------
unwrap_count(path, count(line_no)) <-
    unwrap_hit(path, line_no, _, _),
    changed_file(path);

# The span columns ride the same diag rel via the declared defaults: the
# eprintln rules omit col/end_col, this one supplies them.
diag(
    path: path, line_no: line_no, col: col, end_col: end_col,
    severity: Warning, code: "unwrap-budget",
    msg: "${total} non-test unwraps in a changed file"
) <-
    unwrap_hit(path, line_no, col, end_col),
    changed_file(path),
    unwrap_count(path, total),
    total > 10;

# ---- the gate: ordinary rules, no new syntax ---------------------------
gate_blocked(stage) <-
    diag(severity: severity, code: code),
    diag_stage(code, stage),
    gate_threshold(stage, min_rank),
    severity_rank(severity, rank),
    rank >= min_rank;

gate_exit(stage, 2) <- gate_blocked(stage);
gate_exit(stage, 0) <- gate_threshold(stage, _), !gate_blocked(stage);

any_diag(name) <- program(name), diag(path: _, line_no: _);
check_exit(name, 2) <- any_diag(name);
check_exit(name, 0) <- program(name), !any_diag(name);

? diag(path, line_no, severity, code, msg);                               # I10
```

## The inventions

Twelve constructs the transcription needed and LANG.md does not have. Each one blocks the rail if
removed.

| # | spelling | one-line justification |
|---|---|---|
| I1 | `rel foo(...) from world;` | 139 of 163 corpus files start with extraction; the rel needs a marker saying "no program rule may head this, the link supplies it", which is exactly what makes the tier-1 bind swappable for canned rows with no program-text change |
| I2 | `waiver_line >= line_no - 1` | comparison and arithmetic appear in 166 of 173 corpus files; the eprintln waiver is a range join and is inexpressible without both |
| I3 | `!atom` in a body | negation appears in 112 files; `eprintln_counted` is defined as "hit and not waived" and there is no positive phrasing |
| I4 | `head(group, count(var))` | head-position aggregate, grouping implicit over the remaining head columns; the ratchet compares a count to a number and 76 files aggregate |
| I5 | `diag(path: x, line_no: y, ...)` | the diag product has 7 columns and two rails fill different subsets; positional atoms at that width are unreadable and unmaintainable, and 55 files write this rel |
| I6 | `col: Int = none` in the decl | column defaults are what let both rails head ONE `diag` rel; without them the span-carrying rail and the span-free rail need two rels and the CLI needs two readers |
| I7 | `Key(T)` = FD, statically checked, in tier 0 | tier 0 has no ticks, so "new derivation replaces" is meaningless; the only tier-0 content of a key is "at most one row per key", which is a check over the fact set |
| I8 | `rel program(name: Key(Str));` singleton | "no diagnostics anywhere" is not a row, and datalog has no zero-arity rel; a singleton gives the whole-program negation something to hang on, which is v5's `true()` unit rel rediscovered |
| I9 | `"${var} text"` | 69 files interpolate; the unwrap rail's message is literally `${n} non-test unwraps`, and a diag with no numbers in it is a worse diag |
| I10 | `? head(cols);` plus `--check` exit 2 on a non-empty answer | 130 files end in a `?`; LANG.md's Surface section never lists it, and the CLI gate is defined in terms of it |
| I11 | `_` anonymous wildcard in an atom | `!eprintln_baseline(path, _)` is the no-baseline test; without a wildcard the rule has to invent and discard a variable |
| I12 | several rules may head one rel | v5 unions two extraction rules into `eprintln_waiver_line` and heads `diag` from three rules; LANG.md never states that a rel may have more than one rule |

Inventions I1, I5, I6, I8, I10, I12 are pure surface. I2, I3, I4 add checker obligations
(stratification, arity of the aggregate's group). I7 adds a static FD check. I9 adds a
render-after-join phase to head construction.

## v5 construct to candidate construct

| v5 construct | site | candidate construct | status |
|---|---|---|---|
| `rel eprintln_hit(source_file: file, line_number: int).` | no-new-eprintln:22 | `rel eprintln_hit(path: Path, line_no: Int)` | existed (LANG.md:17, required column types) |
| `scan("WORK", "src/**/*.rs", f, rev)` | :24 | `from world` modifier, tier-1 `bind` | **invented here (I1)**; the bind form is tier 1 and still missing |
| `match_line(f, rev, /eprintln!/, l)` | :25 | absorbed into the same world rel | **still missing** as a body op; regex literals have no candidate spelling |
| `comment(f, rev, /@eprintln-ok:/, l, label: _)` | :32 | `waiver_block_comment` world rel | **still missing** as a body op; the `label:` keyword argument has no candidate form either |
| `ast_yaml(p, rev, :rust, \`...\`, l, c, _, ec)` | rails:118-127 | `unwrap_hit` world rel | **still missing**; the quoted-DSL check is the astgrep lab's subject |
| `waiver_line >= line_number - 1` | :49 | same text | **invented here (I2)** |
| `!eprintln_waived(f, l)` | :56 | `!eprintln_waived(path, line_no)` | **invented here (I3)** |
| `eprintln_count(f, count(l))` | :59 | same shape | **invented here (I4)** |
| `eprintln_baseline("src/config.rs", 1).` fact | :67-70 | bodiless clause | existed (LANG.md:39) |
| the baseline table as a ratchet | :66-70 | `Key(Path)` FD | **invented here (I7)**; `Key` existed (LANG.md:18-21), the timeless reading did not |
| `hits > allowed` | :82 | same text | **invented here (I2)** |
| `diag(path: ..., line: ..., severity: ..., code: ..., msg: ...)` | :73-79 | same named-column form | **invented here (I5)** |
| `col`/`end_col` present on one rail, absent on another | rails:133 vs :101 | declared column defaults | **invented here (I6)** |
| `"...${w}..."`, `"${n} non-test unwraps..."` | rails:17, :133 | `${var}` | **invented here (I9)** |
| `diag_stage(code, stage).` routing facts | :98-101 | bodiless clauses | existed (LANG.md:39) |
| `? diag(path, line, severity, code, msg).` | :103 | `? diag(...)` | **invented here (I10)**; `mode-dominance.md:63-69` types asks but Surface never lists them |
| `changed(p)` / `changed_line(p, l)` | rails:18, :105 | `changed_file` world rel | **invented here (I1)** |
| two rules heading `eprintln_waiver_line` | :30, :40 | rel union | **invented here (I12)** |
| `_` in `!eprintln_baseline(f, _)` | :94 | `_` | **invented here (I11)** |
| `--check` exit 2 | :20 (run line) | `check_exit` rel + CLI reads one row | **invented here (I8, I10)** |
| `enum Severity` (v5 has bare strings) | n/a | `enum Severity { Error, Warning }` | existed (LANG.md:15) |
| recursive `loop_reachable_fn` | rails:84-88 | not transcribed | **still missing**; `checks.pl:32-36` currently rejects self-union, 44 corpus files use it |
| `p != fs:\`src/db.rs\`` path literal | rails:40 | not transcribed | **still missing**; no path literal type in the candidate |
| `call_site`/`nest`/`df_node` builtin rels | rails:63, :68-70 | not transcribed | **still missing**; the builtin-rel catalog is unaddressed at every tier |

Scored over the two rails: 6 constructs already existed, 12 were invented here, 6 remain missing.
The 6 missing ones are all tier 1 (extraction ops, regex and path literals) or tier 2 (recursion,
builtin rels), which is where the topology doc puts them. That part of the topology holds.

## Gate and CLI semantics assumed

The lab assumes and grades this, none of which is language syntax:

- `dl <program> --check` evaluates to fixpoint, runs the `?` asks, and exits **2 if any ask returns
  at least one row**, 0 otherwise. Severity is irrelevant to `--check`. Modelled as
  `check_exit(program, exit_code)`; graded in `clean_state_gate_and_exit_zero` (exit 0) and
  `over_baseline_gate_blocks_commit_only` (exit 2).
- A **stage** gate reads severity. `diag_stage(code, stage)` routes a code to a stage;
  `gate_threshold(stage, min_rank)` sets the bar. `.dl/rails.dl:3` states the agent-turn policy
  ("error severity blocks the agent (exit 2); warn reports without failing"), so
  `gate_threshold("agent-turn", 2)`. The commit threshold of 1 is **this lab's assumption**, not a
  v5 receipt.
- Consequence, graded exactly in `over_baseline_gate_blocks_commit_only`: a warning-severity
  eprintln diag makes `gate_blocked` exactly `["commit"]`, `gate_exit` exactly
  `[("agent-turn", 0), ("commit", 2)]`, and `check_exit` 2. The three gates disagree on purpose and
  the disagreement is data, not code.
- `line_no: 1` on the exceeded diag means "the file head". There is no file-level position in the
  type system, so the rail writes the magic constant 1, exactly as v5 does. See ambiguity A8.

## The ratchet as a functional dependency, and what tier 4 changes

Graded in three checks:

- `ratchet_baseline_present_and_consistent`: the four baseline rows are exactly the v5 table
  (`no-new-eprintln.dl:67-70`), and `fd_violations` over the whole evaluated row set is empty.
- `tightened_baseline_catches_regrowth`: the SAME clean world rows, evaluated against a fact set
  whose `src/daemon/client.rs` baseline is 1 instead of 2, produce exactly one diag,
  `diag("src/daemon/client.rs", 1, warning, "eprintln-exceeded", "2 counted eprintln! hits; the
  grandfathered baseline allows 1. ...", none, none)`, and `check_exit` 2. The loose baseline over
  the identical world produces zero diags, in the same check, as the control.
- `ratchet_fd_rejects_duplicate_key`: a fact set with two `eprintln_baseline` rows for one path
  yields `fd_violation(eprintln_baseline, ["src/daemon/client.rs"])`, and the clean set yields
  none. The FD is checked over the fact set, with no runtime notion of replacement anywhere.

The violation itself is a comparison, not a construct: `hits > allowed` with `hits` from a count
aggregate and `allowed` from the keyed fact table. No ratchet syntax is warranted.

**When tier 4 reinterprets this exact program text, what changes:**

- Nothing in the program text. Zero rules acquire `<+`. The rail has no edge rules at all, so
  rulings R2 (`<+` into a keyed rel), R4 (no edge on departure) and R5 (`delta()` per-atom
  triggers) do not touch it.
- Nothing in stratification, and nothing in the derived row sets at any single repo state. The
  tier-4 engine evaluating this program at one instant must produce the row sets this lab grades,
  byte for byte, or orthogonality claim 1 in the topology doc is false.
- The FD reading of `Key` stays. "At most one row per key" is not weakened by time; tier 4 adds a
  mechanism (replace) that maintains the same invariant instead of rejecting its violation.
- The tightening scenario changes shape. Tier 0 sees two different fact sets and two evaluations;
  tier 4 sees one `eprintln_baseline` row replaced, emitting `-baseline(client.rs, 2)` and
  `+baseline(client.rs, 1)`, with the violation appearing as a `+diag` in the same tick. Same final
  rows, different delivery.
- The fix scenario likewise. `fix_by_waiver_removes_diag_and_violation` computes retraction as
  `ord_subtract(BeforeDiags, AfterDiags)` over two evaluations; tier 4 delivers that subtraction as
  the tick's minus set. Under R7 (tick-boundary diffing) they are required to agree.
- The `?` ask gains a lifetime. In tier 0 it is one snapshot at mode (multi, finite). Under tier 6
  the same text can be a subscription at (multi, until(S)). The program text does not change; only
  the CLI verb does.

**What tier 4 must NOT change:** every check in this lab. The file is a regression fixture for the
tier-4 port, not just a design note.

## Ambiguities found (numbered)

**A1. `count(x)` is bag or set, unresolved.** The interpreter here sorts (group, value) pairs before
grouping, so `count(line_no)` is `COUNT(DISTINCT line_no)`. v5 lowers to SQL `COUNT(...)` over the
grouped rows, which for `unwrap_hit(p, l, _, _)` with two hits on one line counts 2, not 1. Both
rails are insensitive to the difference on real data (one hit per line), which is exactly why this
would ship wrong. Needs a ruling before `emit_ts` grows aggregates. Related to R1 (within-tick
occurrence identity), but distinct: this is a timeless question about projection, not about ticks.

**A2. Interpolation has no expression grammar and no let-binding.** `${hits}` works because `hits`
is a body variable. `${hits - allowed}` is inexpressible, and there is no `let` or `=` binding form
to compute it into a variable first. v5 has `strip_prefix(f, p)` bound with `=`
(`examples/arch-conformance.dl`), so the corpus already needs the form the candidate lacks.

**A3. Named-column atoms in BODY position are underdefined.** `diag(severity: severity, code: code)`
in `gate_blocked` reads 2 of 7 columns. This lab treats every omitted column as a wildcard. That
collides head-on with I6, where an omitted column in HEAD position means "use the declared default".
One syntax, two opposite meanings, decided by position. Either rename one of them or state the rule
loudly.

**A4. A rel may have many rules (I12) but may it mix `from world` rows with rule-derived rows?**
This lab says no, mirroring v5's "one rel = one rule kind" law (which exists because
`rebuild_derived` does a full `DELETE FROM rel`). The candidate has no such engine yet, so the law
is currently unmotivated and unstated. State it or drop it deliberately.

**A5. "Jointly semidet per key" needs a timeless restatement.** LANG.md:56-58 states the law "per
key per tick". `gate_exit` and `check_exit` are each headed by two rules over a `Key` column, and
their disjointness is what makes the FD hold. In tier 0 that is a static disjointness obligation
with no tick in it. The law as written does not apply to a program with no ticks, yet the program
needs it.

**A6. Is `diag` a reserved sink or an ordinary rel?** v5 treats `diag` as an engine-known sink with
a fixed column vocabulary. This lab treats it as an ordinary rel that the CLI happens to `?` and
render, which is strictly simpler and needs no engine knowledge. The two readings differ on whether
a typo in a column name is a type error or a silently different rel. Pick one.

**A7. `Code` and `Stage` are open strings, `Severity` is a closed enum.** Codes are minted per
program, so an enum is wrong for them; but then no exhaustiveness check can cover the
`diag_stage` routing table, and a code with no routing row is silently unrouted. Ruling needed on
whether string-typed columns get a declared domain, or whether unrouted codes are a lint.

**A8. There is no file-level position.** `line_no: 1` on the exceeded diag is a lie that means "the
file head". The `Int` column forces it. Either `line_no` becomes an optional column (which I6 now
makes cheap) or a `Position` type with a file-level case is needed.

**A9. `rev` vanished from the transcription and nothing noticed.** Every v5 source rule binds
`source_rev` and threads it into the extraction op. Tier 0 has no use for it and no place to put it,
so the transcription silently dropped it. That is correct for tier 0 and a trap for tier 1: the bind
has to reintroduce a column the program text never mentions, or extraction re-runs are untriggerable
(AUDIT finding 17, question 3).

## Deviations from LANG.md and from the v5 originals

1. **The v5 message text carried an em dash**, twice. Both messages were rewritten with a semicolon.
   No semantic change; the repo's style law applies to the .md that quotes them.
2. **The exceeded diag gained interpolation** that v5 does not have. v5's message names no numbers,
   so a reader has to go look up the baseline. Adding `${hits}` and `${allowed}` is a deliberate
   improvement and it is what makes `over_baseline_diag_exact_rows` and
   `tightened_baseline_catches_regrowth` grade an exact interpolated string rather than a constant.
3. **`diag_stage("unwrap-budget", "agent-turn")` is added.** `.dl/rails.dl` has no `diag_stage` rows
   at all; the whole file is run by the PostToolUse hook, so the stage is implicit in how it is
   invoked. Making it explicit is what let the gate be data.
4. **The commit-stage threshold is assumed**, see the gate section.
5. **`Path`, `Code`, `Stage`, `Str` are used as column types** with no declaration. LANG.md requires
   column types but declares no primitive type vocabulary anywhere. This lab used names that read
   correctly and moved on; see A7.
6. **The interpreter positionalizes the named-column `diag` head** in declaration order, so a graded
   row reads `diag(Path, LineNo, Severity, Code, Msg, Col, EndCol)`. That is an interpreter
   convenience, not a claim about storage lowering.

## What the grader actually proves

18 checks, exit 0. The four scenarios the task asked for, plus the ratchet sequence and the tier-0
checker claims.

| check | what it pins |
|---|---|
| `stratification_assigns_levels` | the computed assignment, rel by rel: hits and waivers at 0, `eprintln_counted` and `unwrap_count` at 1, `eprintln_count` / `diag` / `gate_blocked` at 2, `gate_exit` / `check_exit` at 3. Negation and aggregation are strictly above their sources, which is what stops an aggregate from accumulating stale counts inside a fixpoint round |
| `stratifier_rejects_negative_cycle` | the same stratifier refuses `unstratifiable(p) <- eprintln_hit(p, _), !unstratifiable(p)`; the checker obligation I3 creates is discharged, not assumed |
| `program_is_timeless` | every rule is `<-`, and no `<+`, `pre`, `now`, `clock`, `tick`, `delta` or `next` term appears in any head or body |
| (a) `clean_state_no_diags` | zero diag rows |
| (a) `clean_state_gate_and_exit_zero` | exact rows: `gate_blocked` empty, `gate_exit` = agent-turn 0 and commit 0, `check_exit` = 0 |
| (a) `waiver_range_join_exact_rows` | **exact row set**: `eprintln_waived` is exactly the one trailing-waived hit; `eprintln_counted` is exactly the other five; `eprintln_count` is exactly the four per-file counts |
| (b) `over_baseline_diag_exact_rows` | **exact row set**: the whole diag product is one row, with path, line 1, `Warning`, `eprintln-exceeded`, the interpolated `3 counted ... allows 2` message, and both span columns at their declared default |
| (b) `over_baseline_count_row` | the count row moved 2 to 3 and the stale 2 is gone |
| (b) `over_baseline_gate_blocks_commit_only` | **exact row set**: `gate_blocked` is exactly `["commit"]`, `gate_exit` is exactly agent-turn 0 and commit 2, `check_exit` is 2. The severity split is real and the three gates disagree correctly |
| (c) `fix_by_waiver_removes_diag_and_violation` | retraction as a set difference: `Before minus After` is exactly the one diag row, `After` diags are empty, gate and exit return to green, and the newly waived hit appears in `eprintln_waived`. The fix is a whole-line waiver on the line ABOVE, which is the tight edge of the range join (174 >= 175 - 1) |
| `new_file_diag_at_hit_line_exact_rows` | **exact row set**: two diags at lines 203 and 288, not at line 1 |
| `new_file_no_exceeded_diag` | the two diag rules are mutually exclusive; the unbaselined file has a count row but no exceeded diag |
| (d) `unwrap_aggregate_and_interpolation` | `unwrap_count("src/engine.rs", 12)`, 12 diag rows, and two of them checked in full including the rendered `"12 non-test unwraps in a changed file"` and the span columns 8 and 16 |
| (d) `unwrap_unchanged_file_silent` | the `changed_file` join sits inside the aggregate, so an unchanged file with the same 12 hits produces no count row and no diag |
| (d) `unwrap_below_budget_silent` | 3 hits under a budget of 10 produce a count row and no diag |
| `ratchet_baseline_present_and_consistent` | the exact four-row v5 baseline table, and zero FD violations over the full evaluated set |
| `tightened_baseline_catches_regrowth` | same world, tightened fact set, exactly one diag with the correct interpolated numbers; the loose control produces zero |
| `ratchet_fd_rejects_duplicate_key` | one FD violation naming the rel and the offending key; the clean set has none |

Exact row sets (not counts) are graded in `waiver_range_join_exact_rows`,
`over_baseline_diag_exact_rows`, `over_baseline_gate_blocks_commit_only`,
`new_file_diag_at_hit_line_exact_rows`, `clean_state_gate_and_exit_zero`,
`unwrap_unchanged_file_silent`, `ratchet_baseline_present_and_consistent` and
`tightened_baseline_catches_regrowth`.

## What this earns the tier order

- **T0's scope line in the topology doc is short by six constructs.** Add comparison and arithmetic,
  string interpolation, named-column atoms, column defaults, singleton rels, and rel union to T0
  before anything is implemented against it. All six are cheap; none is optional.
- **T3 should be folded into T0 plus a CLI section.** It contributed no syntax. Keeping it as a tier
  implies an implementation phase that does not exist.
- **T1's job on this rail is exactly one thing:** turn `from world` into `bind`. The program text
  does not move. That is a small, well-scoped, testable milestone and it is the whole distance
  between this lab and a v5 lint rail running under v6 semantics.
- **Recursion and the builtin-rel catalog are the next real gap**, not time. `.dl/rails.dl`'s other
  half (`loop_reachable_fn` over `call_edge`) was left untranscribed because both are missing, and
  `checks.pl:32-36` actively rejects the self-union that 44 corpus files use.
- **A1 (bag or set aggregation) should be ruled on alongside R1**, since both are about
  multiplicity, but it must be ruled on separately: A1 bites in the timeless fragment where no tick
  exists to blame.
