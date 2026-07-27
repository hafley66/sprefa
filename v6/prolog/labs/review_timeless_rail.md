# Review: timeless_rail lab

Reviewer run: `swipl -q -l v6/prolog/labs/timeless_rail.pl -g go -g halt` observed 18 PASS, exit 0,
matching the lab's claim (timeless_rail.md:3). Transcription checked line by line against
`.dl/no-new-eprintln.dl` and `.dl/rails.dl:107-134`; the baseline table, waiver range join,
negation shape, and unwrap aggregate are faithful, and all six deviations the .md declares
(timeless_rail.md:346-363) check out as declared.

## Invention-by-invention

| invention | verdict | audit reconciliation | blocking receipt honest |
|---|---|---|---|
| I1 `from world` | new in this position, but it is the killed `source` keyword returning as a rel modifier (LANG.md:15 killed it with reason "inference or unbundling"; this is the unbundling). The .md's v5-mapping table (timeless_rail.md:205) never says so | predicted: AUDIT finding 17 (extraction absent, blocker) and keep/kill row "extraction ops: add" (AUDIT.md:758). Resolution option 3 of finding 17 ("the language only consumes output rels") is exactly this marker. No KILL contradicted; the `source` kill was LANG.md's own, and its stated reason permits the respelling | yes, with a caveat: the rail is writable with NO marker (an unheaded rel is inferable as world-fed); the marker buys the typo check, not expressibility |
| I2 comparison/arithmetic | new | predicted: AUDIT keep/kill "comparison / arithmetic: add, 166 of 173" (AUDIT.md:756) | yes; the waiver range join (timeless_rail.pl:113-117) has no phrasing without both |
| I3 `!atom` | new | predicted: AUDIT "negation: add" (AUDIT.md:754). Corpus number cited (112) is from the 163-file topology census, while I2/I4/I9/I10 cite the 173-file AUDIT census; mixed sourcing, see errors list | yes; "hit and not waived" has no positive form |
| I4 head-position `count` | new | predicted: AUDIT "aggregation: add, 76 files" (AUDIT.md:753) | yes; the ratchet compares a count to a number |
| I5 named-column atoms | new, but it is TWO constructs sharing one spelling: record construction in heads, record pattern in bodies. The lab's own A3 (timeless_rail.md:309-314) states the collision and asks for a ruling; the invention count treats it as one | partially predicted: AUDIT saw the shape only as the diag sink ("diag ... sinks: add, 55 files", AUDIT.md:761); the generalization to any rel is the lab's | yes for the head form (a 7-column positional diag is the alternative); the body form at 2-of-7 columns (timeless_rail.pl:214) is convenience, not blocking, since positional with wildcards works |
| I6 column defaults | collapse into Option-typed columns. `col: Int = none` puts the atom `none` in an `Int` column, ill-typed under LANG.md:17's required column types; the honest type is `Option(Int)`, which is the EXISTING enum machinery, and the default is then separable sugar. The lab's own .pl writes `none, none` explicitly in both span-free rules (timeless_rail.pl:147-150, 168-170), demonstrating the rail runs without defaults | not predicted by AUDIT anywhere | no. The I6 justification "without them the span-carrying rail and the span-free rail need two rels and the CLI needs two readers" (timeless_rail.md:188) is false; explicit `none` in one rel suffices |
| I7 `Key(T)` as static FD | instance: a new tier-indexed READING of an existing construct (LANG.md:18-21), zero new syntax. Code supports the reading: `fd_violations/2` (timeless_rail.pl:402-411) is the only consumer of `keyed/2`; the evaluator (timeless_rail.pl:253-281) never consults it | AUDIT kept Key ("keep: a real win", AUDIT.md:729); the FD-only tier-0 payoff is new. No contradiction with the merge lab, see tier section | yes as a check, and the lab is candid that the ratchet works identically unkeyed (timeless_rail.md:33-36) |
| I8 singleton rel | instance, and a pattern rather than a construct: `rel program(name: Key(Str)); program(...)` uses only existing constructs (rel decl, Key, fact). Zero grammar added | predicted: AUDIT finding 16's gen-reference row lists "the `true()` unit rel" as a missing construct (AUDIT.md:633). The lab's "v5's true() rediscovered" matches | yes as an idiom (whole-program negation needs an anchor, timeless_rail.pl:236-243), but it does not belong on a list of surface constructs |
| I9 `${var}` interpolation | new | predicted: AUDIT "string interpolation: add, 69 files" (AUDIT.md:757) | yes for the unwrap rail (`${n}` at rails.dl:133 is a v5 receipt); no for the eprintln rail, where v5's messages carry no numbers (no-new-eprintln.dl:78,91) and the lab added interpolation as a disclosed improvement (timeless_rail.md:350-353) |
| I10 `?` ask + exit 2 | instance of a known gap: mode-dominance already types asks (per timeless_rail.md:219); the topology T0 line already says "snapshot asks" (tier-topology.md:32). What is new is only the exit-code convention as rows | predicted: AUDIT 18c ("`? query` has no home in the surface", AUDIT.md:694-696) and keep/kill "add, 130 files" | yes; the CLI gate is defined in terms of it |
| I11 `_` wildcard | new, trivially; standard datalog surface nobody wrote down | not predicted by AUDIT | yes, narrowly: `!eprintln_baseline(path, _)` (timeless_rail.pl:172) needs it or an invented discarded variable |
| I12 several rules per head | not a construct; a permission sentence for LANG.md. Plain multi-rule heads are default datalog. The adjacent real questions are checks.pl's self-union rejection (AUDIT finding 2, recursion, out of this lab's scope) and A4 (world+derived mixing, which the lab defers with disclosure, timeless_rail.md:315-319) | partially predicted: AUDIT finding 5 covers the level/edge MIX; plain level union appears nowhere in AUDIT as a gap | yes as semantics (the waiver union and the three diag rules need it), no as syntax; removing it removes a sentence, not a production |

No AUDIT KILL verdict is contradicted. The kills (`->` merge, `<+` as specified, one-time-cut,
any-atom firing, `in` fan-out) all concern constructs this lab never touches; the program is
level-only by design and `program_is_timeless` proves it mechanically.

Cardinality recount: of the claimed 12, genuinely new grammar is 8 at most (I1, I2, I3, I4, I5 as
two constructs or one with a stated position rule, I9, I10, I11), of which I11 is trivial and I10
half-exists. I6 reduces to Option typing plus optional sugar. I7 is a reading, I8 an idiom, I12 a
sentence. "Each one blocks the rail if removed" (timeless_rail.md:178) survives for semantics but
not for syntax: I6, I8, I12 are removable as constructs with the rail intact.

Direct answers to the three scrutiny questions: named-column atoms are two constructs, and the
default-in-head half is not needed; with Option-typed columns and explicit `none`, omission in a
head can simply be an error, which deletes the A3 collision (body omission = wildcard, head
omission = error). Column defaults and the singleton rel are unrelated; neither is sugar over the
other; the singleton is not a construct and defaults are sugar over Option. Rel union is just
permitting multiple rules per head; it warrants a sentence in LANG.md, not a construct slot.

## Tier verdict

**Claim (a), T0 scope grows by six constructs: direction confirmed, count inflated.** Re-derived
from the lab's own evidence: the T0 line (tier-topology.md:31-32) must gain comparison/arithmetic
guards (166/173 corpus receipt), string interpolation in head strings (69), and named-column atoms
with the head/body position rule stated (55 diag-shaped files). That is three constructs. The
remaining three items on the lab's list are a type-vocabulary decision (Option-typed columns,
subsuming defaults), and two spec sentences (multi-rule heads are legal; the unit-rel idiom for
whole-program negation). All six belong in the T0 SCOPE TEXT before implementation, so the
amendment is warranted; calling all six "constructs" overstates the grammar cost by half. Side
note the lab's arithmetic never quite closes: "covers only 6" of 12 against an uncovered list of 6
leaves I1 and I11 in neither set (timeless_rail.md:22-24).

**Claim (b), T3 collapses into T0 plus a CLI convention: proven, but it is a sharpening, not an
amendment.** The topology doc already titles T3 "(convention, not syntax)" and says "mostly
library + CLI" (tier-topology.md:50-53). What the lab adds is proof that even the parts a reader
might assume need engine support (gate, severity split, stage routing, exit codes) are rules over
fact tables: timeless_rail.pl:213-243, six clauses, using nothing past the T0 additions of claim
(a). The T3 row should be RENAMED, not deleted: keep it as a scheduling milestone whose
deliverables column becomes "std/diag library (diag decl, severity_rank, gate_threshold,
gate/check_exit rules) + CLI verbs (--check exit 2 on rows, stage gates, LSP span rendering)" and
whose checkers column is empty. Deleting the row loses the 54-file payoff milestone and the
`<- T0` dependency the shortest-path note relies on (tier-topology.md:110). One gate on the
collapse: it assumes ambiguity A6's ordinary-rel reading of `diag` (timeless_rail.md:325-329); if
`diag` becomes an engine-known sink, some T3 content moves back into the engine. Rule A6 before
rewriting the row. Two disclosed assumptions ride along: the commit threshold of 1 is the lab's,
not v5's (timeless_rail.md:355-356), and rails.dl has no `diag_stage` rows at all (stage is
implicit in the PostToolUse hook), so routing-as-data is the lab's design choice, working but not
corpus-forced.

**Claim (c), Key(T) buys only static FD rejection in tier 0: supported by the code, no conflict
with the merge lab.** `keyed/2` (timeless_rail.pl:83-88) feeds only `fd_violations/2`; evaluation
is key-blind. The consolidation doc's pro-Key receipts (lab-consolidation.md:74-80, "all keyed
behavior off column positions"; PROVEN 6, keyed due-row replace as debounce) are tier-4/5 receipts
about replace semantics and demand identity. Different tiers, one declaration: Key(T) reads as
static FD at T0, replace-maintaining-the-same-FD at T4, demand identity at T5. The lab's own
tier-4 section states the compatibility condition correctly (timeless_rail.md:279-281: replace
maintains the invariant T0 rejects violations of), which is orthogonality claim 1 holding. The
Key-vs-`->` user decision in the consolidation doc is untouched by this lab (no effects appear).

## A1 versus R1

Distinct. R1 (lab-consolidation.md:32-36) is about occurrence identity WITHIN A TICK: two
same-tick arrivals collapse in a set before an edge fold sees them; its candidate mechanisms
(engine-stamped seq column, Z-set multiplicities) are tick machinery. A1 is reachable with no tick
anywhere in the program: `count(line_no)` over `unwrap_hit(path, line_no, _, _)` asks whether the
aggregate counts distinct projected values or distinct body derivations. The lab's interpreter
dedupes projected pairs (`sort(Pairs0, Pairs)`, timeless_rail.pl:290-291), giving
COUNT(DISTINCT); v5's SQL lowering counts join rows, giving 2 for two hits on one line. The 18
checks cannot see the difference because `unwrap_run` mints all-distinct lines
(timeless_rail.pl:505-509), which confirms "would ship wrong invisibly". A Z-set resolution of R1
would settle A1 as a corollary (aggregates over multiplicities); a seq-column resolution would
not. So the consolidation doc needs a separate ruling.

Proposed R8: AGGREGATE MULTIPLICITY IS UNDEFINED (timeless_rail A1): `count(x)` in a head must be
ruled bag-of-derivations (count the body's join result; v5-SQL-compatible; two hits on one line
count 2) or set-of-projected-values (the lab interpreter's behavior; DISTINCT becomes the only
mode). The reference interpreter, lowerSql, and emit_ts must implement the same answer, and the
lab's grader should gain a two-hits-one-line world row so the choice is graded rather than
invisible. BLOCKS: emit_ts/lowerSql aggregate work. Interacts with R1 only if R1 resolves via
Z-sets. The choice itself is the user's; the v5-compatibility argument favors bag, the lab's
interpreter currently implements set.

## Wrong or overclaimed

1. I6 blocking justification false: "need two rels and the CLI needs two readers"
   (timeless_rail.md:188). Explicit `none` in one Option-columned rel suffices; the .pl does
   exactly that (timeless_rail.pl:147-150, 168-170).
2. `col: Int = none` is ill-typed as declared (timeless_rail.md:73 against LANG.md:17). A8 nearly
   reaches the fix (optional column) but the declaration as written puts a non-Int in an Int
   column and no ambiguity entry flags it.
3. Invention count inflated: I8 and I12 add zero grammar (see table); "12 surface inventions" is
   at most 8 constructs plus a reading, an idiom, a sentence, and a collapsible sugar.
4. I1's table row (timeless_rail.md:205) omits that `from world` re-derives the killed `source`
   keyword in modifier position.
5. Mixed corpus censuses in one table: I3 cites 112 (163-file census, tier-topology.md:15) while
   I2/I4/I9/I10 cite the 173-file AUDIT census (timeless_rail.md:184-192). Both are real numbers;
   citing them side by side without sourcing invites a false precision.
6. The T3 claim is presented as amending the topology doc (timeless_rail.md:28-32) when the doc's
   own row already says "convention, not syntax" (tier-topology.md:50); the lab proves and
   sharpens an existing position.
7. "covers only 6" arithmetic does not decompose against the 12-item list
   (timeless_rail.md:22-24); two inventions fall in neither the covered nor the uncovered six.

## Disposition

Accept with notes. The lab is green as claimed (18 PASS re-verified), the transcription is
faithful with every deviation disclosed, the tier-4 regression-fixture framing is the right
contract, and the three headline findings survive independent re-derivation in weakened form:
T0's scope line does need the six additions but only three are constructs, T3 collapses to a
library-plus-CLI row (rename, do not delete, and rule A6 first), and Key-as-FD is code-supported
and compatible with the merge lab's receipts. Before the .md is banked into LANG.md or the
topology doc, fix the I6 justification and the `Int = none` typing, restate I8/I12 as idiom and
spec sentence rather than constructs, and add R8 to the consolidation doc with a
two-hits-one-line graded row in this lab as its fail-pre-fix test. No rework of the .pl is
needed for any of that except the one added world row.
