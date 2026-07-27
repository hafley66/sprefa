# review: expressions lab (post-lab review, 2026-07-27)

Reviewed: `v6/prolog/labs/expressions.pl` + `expressions.md` against `LANG.md`, `AUDIT.md`,
`plans/2026-07-27-lab-consolidation.md`, `plans/2026-07-27-tier-topology.md`, and the concurrent
`timeless_rail.md`/`timeless_rail.pl` (neither lab saw the other).

Verified by running: `swipl -q -l v6/prolog/labs/expressions.pl -g go -g halt` printed 22 PASS
lines, exit 0. The .md's claimed count (expressions.md:4) matches the observed run.

## 1. Construct cardinality

| construct | verdict | audit reconciliation |
|---|---|---|
| `:=` bind | new operator, warranted | AUDIT.md:755 adds comparison/arithmetic but never a binding form; timeless_rail A2 (timeless_rail.md:306-308) independently demanded one. `:=` resolves A2. Keyword count stays 4 (expressions.md:41-42 checked against the grammar) |
| `==` compare | instance of AUDIT.md:755 "comparison: add" | the novelty is splitting v5's one mode-polymorphic `=` into two operators; port cost assessed below |
| `quote(...)` | new construct, zero corpus occurrences; accept the RULE, mark the TYPE blocked | the evaluation-default rule is the lab's real contribution; `quote`'s result type `Term` is the lab's own stand-in for unspecified struct columns (expressions.md:437-440), so the construct is ruled in against a type whose existence waits on 18a (AUDIT.md:681-687). If structs replace `Term`, `quote`'s target type must be respecified |
| `${...}` interpolation | instance of AUDIT.md:757 "string interpolation: add"; convergent with rail I9 | name-only holes + desugar-to-concat (expressions.md:189-194) is stricter and better specified than the rail's untyped "render-after-join phase" (timeless_rail.md:198) |
| `Display` closed set | keep, restate as a whitelist, drop the word "class" | implementation is two facts, `displayable(int)`/`displayable(str)` (expressions.pl:52-53); see scrutiny (b) |
| stdlib (13 names, 14 rows) | instance of AUDIT.md:756 "scalar functions: add" (28 files) | each entry corpus-receipted except `apply` (0 occurrences, cut recommended below) and `digest` (blocked on 18a, the lab says so at expressions.md:409-410). `strip_suffix` is symmetry-only, no occurrence count given (expressions.md:270); harmless |
| evaluation-default collision rule | new semantic rule, sound as graded | both error polarities carry graded receipts (`bare_arithmetic_into_term_column_rejected`, `quoted_term_into_int_column_rejected`, observed PASS); the circularity justification is overclaimed, see scrutiny (a) |

### `:=`/`==` vs v5's mode-polymorphic `=`: does any corpus line break

No line breaks; every line classifies. Grep basis: 50 of 210 repo-proper .dl files contain `=`
bind/compare goals. The classification is decidable left to right by exactly the rule the lab's
checker already uses (bound iff previously bound, expressions.pl:255-259, 290-293). The receipt
that shows both meanings adjacent is `bench/flow/flow_scip.dl:34`:
`ext = split(f, ".", -1), ext = "rs".` which transcribes as
`ext := split(f, ".", -1), ext == "rs"`. Pure-compare receipt:
`v6/dl/fixtures/conformance.dl:52` (`value = 10`, value bound by the preceding atom, becomes `==`).
Bind-with-arithmetic receipt: `std/suppress.dl:257` (`e = dl + 1`, becomes `:=`). The port is a
mechanical one-pass rewrite, and the pass is the same binding-order scan a transcriber must run
anyway to check range restriction. One caveat: rebinding is an error under `:=`
(expressions.pl:257), and v5's `=` never rebinds either (second `=` on a bound name is a compare),
so no corpus shape is lost.

### `+`-is-Int-only: corpus safety of dropping str+str

Grep over repo-proper .dl files: 9 string-concat `+` sites in 4 files.

- `examples/chaos-soak.dl:121` (`"dl://rel/" + name`), `:237` (msg concat)
- `examples/type-from-json.dl:67` (`"partial_" + rel`, head position)
- `examples/arch-expr.dl:81, 85, 90` (`child_txt + " >> " + rest` etc.)
- `examples/callable-coverage.dl:70, 75, 111` (msg concats)

Every hole in all 9 is a plain variable, so every site rewrites to name-only interpolation
(`"dl://rel/${name}"`, `"${child_txt} >> ${rest}"`). Matches the lab's claim that interpolation
covers concatenation (expressions.md:174-177). Excluded from the count: regex character classes
(`std/suppress.dl:101`, `examples/autodoc-plans.dl:26`), the literal `"L4+"`
(`examples/experiment-auto-arch.dl:70`), and jq text inside shell backticks
(`examples/npm-crawl.dl:48`). Verdict: corpus-safe, 9-site mechanical rewrite. Note v5 documents
the overload at README.md:377 and README.md:1100; both lines become stale on port.

## 2. timeless_rail spelling divergences (the parallel-universe check)

| construct | rail spelling | expressions spelling | recommended winner | why |
|---|---|---|---|---|
| body comparison | `waiver_line >= line_no - 1` bare, ops `>= <= >` only, untyped (I2, timeless_rail.md:109-110, 185) | same text, full op set `< <= > >= == !=`, Int-both-sides typing (expressions.md:48, 53-55) | expressions | superset with typing rules and graded rejections; the rail's spelling is a subset, byte-compatible |
| arithmetic in comparison sides | `line_no - 1` (timeless_rail.md:109) | identical (expressions.md:53) | convergent, no reconciliation | same text in both labs |
| equality compare | never spelled (the rail programs use no equality goal) | `==` / `!=` | expressions by default | nothing to reconcile; rail inherits `==` |
| binding form | absent; A2 records the gap ("no `let` or `=` binding form", timeless_rail.md:306-308) | `name := expr` (expressions.md:56-57) | expressions | resolves rail A2 exactly; `${hits - allowed}` becomes `gap := hits - allowed` then `${gap}` |
| interpolation | `"${hits} counted..."` in named-column heads, semantics = "render-after-join phase" I9 (timeless_rail.md:124, 191, 198) | `${name}` holes, desugar to `concat`, Display typing, Int auto-convert (expressions.md:64-65, 189-194) | expressions for semantics, rail for the sites | hole grammar is identical (`${var}`, names only) in both labs, so no spelling conflict; expressions supplies the typing the rail left undefined. Reconciliation item: state in one place that holes stay name-only forever and `:=` is the computed-hole answer, closing rail A2 |
| aggregate in head | `eprintln_count(path, count(line_no))` I4 (timeless_rail.md:116, 187) | not in scope, but the head grammar `head ::= rel "(" expr,* ")"` (expressions.md:50) plus the checker's function lookup (expressions.pl:208-215) parses `count(line_no)` as an application and throws `unknown_function(count/1)` | rail's spelling stands; expressions' checker must carve out | REAL DIVERGENCE. Reserve `count`/`sum`/`min`/`max` as head-position aggregate forms excluded from stdlib lookup. Affects 63 count files (tier-topology.md:15), so this is not an edge case |
| named-column heads carrying expressions | `msg: "${total} non-test unwraps..."` I5+I9 (timeless_rail.md:151-156) | heads are positional only (expressions.md:50); no named-column form exists in the lab | rail's named-column form, with expressions' per-column typing applied to each value | the two grammars must merge: named-column head where each value is an `expr`. Also interacts with rail A3 (omitted column = default in head, wildcard in body, timeless_rail.md:315-318); the merged grammar has three occupants per head column (expression, omitted-default) and per body column (term, wildcard, omitted-wildcard) and should be written down once |
| `_` wildcard | I11, `!eprintln_baseline(path, _)` (timeless_rail.md:193) | ambiguity 8: `_` binds nothing and an expression referring to it is an error (expressions.md:357-361, expressions.pl:169-170) | convergent | both labs independently note LANG.md's silence on `_`; expressions adds the expression-position ruling, keep it |
| Int-into-Str interpolation | rail interpolates `${hits}`, `${allowed}`, `${total}` (Int vars) with no stated rule | graded auto-convert (`interpolation_auto_converts_int`) | convergent | expressions supplies the rule the rail assumed |

Net: one hard divergence (aggregate heads vs the head-expression grammar), one grammar merge
(named-column heads), the rest convergent with expressions supplying the typing the rail assumed.
The two labs did not contradict each other on any spelling both actually wrote, which is a good
sign for the surface.

## 3. Tier verdict

**Tier 0 placement confirmed, with the recursion ruling promoted to a named T0 checker
obligation.**

Re-derivation of the recursive-head-arithmetic rejection: the ban is only enforceable by a
component that sees both the rule dependency graph and the expression forms in heads, and that
component is the T0 stratifier. The lab's own check covers direct self-reference only
(expressions.pl:329-336, comment at :327-328 concedes "the mutual case wants the stratum SCC").
timeless_rail already built a T0 stratifier that computes level assignments and rejects negative
cycles (`stratification_assigns_levels`, `stratifier_rejects_negative_cycle`,
timeless_rail.md:372-374). These are the same component. Consequences for
`plans/2026-07-27-tier-topology.md`:

- T0's checker line (tier-topology.md:34, "HM/enum typing, exhaustiveness, stratification") grows
  one entry: head-expression ban inside recursive SCCs. Nothing moves between tiers; "recursion is
  already T0" (tier-topology.md:46) stays true and this is why the ban cannot live anywhere else.
- Corollary the topology doc should state: the T0 stratifier's input grammar includes expressions.
  Expressions cannot be bolted onto a frozen T0 checker later; they are part of T0's checker
  input, exactly as the review question suspected.
- The lab's `pre`-breaks-the-stratum note (expressions.md:401-405) is a T4 interaction stated
  early and correctly; no T4 change needed.

**`apply`: cut from the stdlib now.** Grounds: (1) statically unsound by the lab's own ambiguity 2
(expressions.md:317-323), `apply(quote("x"), 1)` types as Int and evaluates to a string, the only
unsound form in the layer; (2) zero corpus occurrences, the optimistic-update pattern is
aspirational; (3) its dependency (a canonical term encoding) waits on 18a regardless. The sound
alternative is the lab's own third option plus 18a: typed patch structs. `struct Delta { amount:
Int }`, `pending(id, delta) <- queued(id, delta)`, and the consumer is an ordinary rule
`optimistic(id, value + amount) <- pending(id, Delta { amount }), base_value(id, value);` using
the `Entry { tag, .. }` pattern form LANG.md:37 already has. Statically typed, no `it`, no generic
apply. Honest cost of the cut: optimistic update was `quote`'s "one worked receipt"
(expressions.md:292-293), so cutting `apply` leaves `quote` with no stdlib customer today. `quote`
still stands as the collision rule's storage half, but its .md should stop leaning on the
optimistic-update example once `apply` goes, and the `it` reserved name (expressions.pl:239-241)
goes out with `apply`.

`digest` stays as typed-and-stubbed, exactly as the lab states (expressions.md:409-410); its
runtime is a placeholder pending 18a and it blocks nothing.

## 4. Lab-specific scrutiny

**(a) The circularity claim is asserted, not demonstrated, and under the lab's own assumptions it
is false as stated.** expressions.md:106-109 claims "The cycle is real, not hypothetical: HM types
are holes during inference, and a column whose type is still a variable at that moment has no
answer." No graded check demonstrates it (none of the 22 touches type-directed elaboration; the
.pl carries the claim only as comments at expressions.pl:48-50 and :180-181). More important:
column types are REQUIRED and ground in this design (LANG.md:17; `rel_decl` facts at
expressions.pl:94-124), and the lab's own ambiguity 3 leans on exactly that ("the argument's type
is always known by the time the call is typed", expressions.md:326-330). With ground column
types, a type-directed elaborator could consult the declared column type before inference runs;
no hole ever reaches a column unless generic rels exist, which the same ambiguity says they do
not. The rejection of type-directed disambiguation still stands, but on its FIRST bullet (same
text means two things, decided by a declaration elsewhere; non-local, unreadable) and on
future-proofing against generic rels. Fix: rewrite the second bullet to say the cycle appears if
column types ever become inference holes (ambiguity 3's exact scenario), and delete "real, not
hypothetical".

**(b) `Display` is acceptable at the price paid, provided it is restated as a closed whitelist.**
There is no contradiction with rejecting Term interpolation once the rule is stated correctly:
conversion is permitted exactly for types with a language-defined canonical rendering (Int has
one decimal form); it is refused for types whose rendering would have to be invented (Term,
enum, struct; the `[object Object]` argument at expressions.md:184-187). Those are one rule, not
a rule and an exception. The implementation already is the closed set, two facts at
expressions.pl:52-53. The .md's framing "a class the language has" (ambiguity 4,
expressions.md:332-338) invites type-system machinery the design does not need; restate as: the
displayable set is closed, currently {Int, Str}, not user-extensible, and any future addition is
a spec change. With that restatement ambiguity 4 closes.

**(c) min/max: rule now, as follows.** The corpus numbers are on one side only: 36 aggregate
occurrences (21 `max(`, 15 `min(`, expressions.md:280-282; receipt
`examples/gh-cache.dl:99` `resp_latest(ep, max(b))`), zero scalar occurrences. Ruling to
inherit: `min`/`max` (with `count`/`sum`) are reserved aggregate forms, legal only in head
columns, excluded from the stdlib function namespace and from the expression grammar. No
arity-directed or type-directed dispatch, which the lab is right to distrust (expressions.md:
313-315). If a scalar two-argument form ever earns a corpus receipt, it enters the stdlib under
distinct names (`least`/`greatest`, the SQL precedent), never under `min`/`max`. This same
reservation is the fix for the `count(line_no)` head divergence in section 2, so one ruling
closes both.

## 5. Wrong or overclaimed (receipts)

1. **"Expressions ... constrain none of them" is false as written** (expressions.md:414-418,
   claiming the only aggregate interaction is the min/max naming collision). The head grammar
   (expressions.md:50) plus the checker's application rule (expressions.pl:208-215) makes every
   aggregate head parse as a function application and throw `unknown_function`; that constrains
   63 count-using files (tier-topology.md:15), not just a future scalar min/max. Fixed by the
   section-4c reservation, but the .md sentence overclaims orthogonality.
2. **The circularity claim** (expressions.md:106-109), section 4a above.
3. **Function/rel namespace collision is unlisted.** `expr_type` throws
   `rel_read_in_expression` whenever a name+arity matches a rel declaration
   (expressions.pl:208-212), so a user rel named `split/3` or `len/1` silently poisons the
   stdlib name. Not in the 8 ambiguities. Needs a line: stdlib names are reserved rel names, or
   the lookup order is defined the other way.
4. **Partiality of pure functions is unstated and inconsistent in the reference interpreter.**
   Out-of-range `split` fails the goal and silently drops the row (expressions.pl:461-465 via
   `nth0` failure), which happens to match v5's documented NULL-filter (README.md:378); division
   by zero throws (expressions.pl:377, `//`). The .md calls the stdlib "pure term to term"
   (expressions.md:35) and never says what a partial application does in a `:=` or a head.
   Ambiguity 7 covers truncation only. Needs a stated rule (row-drop vs error), because the two
   behaviors shipped differ.
5. **`%` vs `mod` deviation unlisted.** The surface grammar says `%` (expressions.md:47); the
   lab declares `arith_op((mod))` (expressions.pl:79). Every other reader-driven respelling is
   in the deviations section (expressions.md:420-431); this one is missing. Cosmetic.
6. **"~15-entry stdlib" per the tasking is actually 13 names / 14 table rows** (expressions.md:
   256, verified against expressions.pl:59-71 which holds 13 `stdlib/3` rows plus `concat` as a
   special clause). The .md's own count is accurate; recorded here so nobody re-counts.
7. **Corpus file-count basis differs across docs, inherited not introduced**: expressions.md:10
   says "166 of 173"; tier-topology.md:3 says 163 programs. Both cite the same survey. Whoever
   consolidates should pick one denominator.

## 6. Disposition

Accept with notes. The core deliverable is sound and verified: 22/22 PASS observed, both
collision polarities rejected by ordinary typing with graded error terms, the `:=`/`==` split
survives the corpus (50-file `=` usage transcribes mechanically, receipt flow_scip.dl:34), and
the `+`-Int-only deviation is corpus-safe at 9 rewritable sites in 4 files. The parallel
timeless_rail lab contradicts it on no spelling both labs actually wrote; the two real
reconciliation items are aggregate-head reservation (section 4c ruling closes it) and the
named-column-head grammar merge. Required edits before the lab banks: cut `apply` and the `it`
reserved name (ship the typed-patch-struct alternative note), soften the circularity paragraph
to the honest locality argument, restate `Display` as a closed whitelist, add the
namespace-collision and partiality rulings to the ambiguity list, and fix the "constrains none
of them" sentence. Tier 0 placement stands, with the recursion ban recorded as a T0 stratifier
obligation and one line added to the topology doc's T0 checker list.
