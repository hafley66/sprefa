# `:=` composition verdict (opus lab, 2026-07-29)

Base `b535ca62`. Battery unmoved: conformance 163 PASS, plunit 140/140,
TEXT_DOOR 102/102/0, sweep 102 compiled / 100 identical / 0 wrong.

**Question.** The `:=` bind goal is a construct the user never saw or ruled
(plans/2026-07-27-aggregate-analysis.md:35 names it "new operator", :104 records
`=` killed and split into `:=` / `==`). User instinct: "in rxjs, assignment is
just a next"; "i want match/scan/slice style technique, not assign. assign is
fine but its gotta be a shorthand here."

**Answer, one line.** Assignment already composes out of what ships. `:=` and
the argument-position expression reach the SAME compiler
(`lower.pl:compile_expr/4`) and the SAME evaluator (`body.pl:eval_expr/2`), and
the two spellings emit **byte-identical TypeScript modules**. `:=` buys exactly
one thing today: a column name on an *undeclared* head rel. It is already a
shorthand; it is not a construct.

---

## 1. Corpus census

30 real use sites. `:=` appears in 31 files but 24 of those only declare the
operator; 7 conformance fixture files plus `v6/dl/fixtures/flagship-flow.dl6`
actually use it. `1_host_expand.pl:373` is compiler-generated, not authored.

| class | rx shape | sites | where |
|---|---|---|---|
| (a) pure per-row compute | `map` | 14 | see below |
| (b) `pre`-fed accumulation | `scan` | 15 | see below |
| (c) naming-for-reuse (head **and** guard) | `map` + `filter` over one const | 1 | `expressions.pl:165` |
| (d) anything else | — | **0** | — |

### (a) map, 14 sites — 7 of which DISSOLVE

| file:line | bound name | shape | fate |
|---|---|---|---|
| `expressions.pl:189` | `Text` | `concat([...])` | genuine |
| `expressions.pl:206` | `Quotient` | `Numerator / Denominator` | genuine |
| `expressions.pl:207` | `Remainder` | `Numerator mod Denominator` | genuine |
| `expressions.pl:227` | `Sum` | `Value + 1` | genuine |
| `operators.pl:14` | `Out` | `Value * 2` | genuine |
| `json_arm.pl:14` | `Value` | `{stars: 4, name: Name}` | compiler-refused either spelling |
| `json_arm.pl:179` | `Doc` | `{langs: Langs}` | compiler-refused either spelling |
| `flagship-flow.dl6:82` | `from` | `concat([path,':',from_start,':',from_end])` | **DISSOLVES under file_span** |
| `flagship-flow.dl6:83` | `to` | same shape | **DISSOLVES** |
| `flagship-flow.dl6:129` | `arg` | same shape | **DISSOLVES** |
| `flagship-flow.dl6:130` | `param` | same shape | **DISSOLVES** |
| `flagship-flow.dl6:135` | `ret` | same shape | **DISSOLVES** |
| `flagship-flow.dl6:136` | `call` | same shape | **DISSOLVES** |
| `flagship-flow.dl6:159` | `node` | same shape | **DISSOLVES** |

The seven dissolving sites are the whole `:=` population of the flagship
program, and every one is the same idiom: hand-building a node identity by
concatenating `path`, `start`, `end`. plans/2026-07-29-file-span-design.md:8-9
names that concat as the missing file reference itself. Under `file_span` the
span **is** the identity, so the concat has nothing left to compute. That is
one full use class disappearing for reasons that have nothing to do with this
lab, and it moves the honest genuine-map count from 14 to **5**.

### (b) scan, 15 sites

`merge_family.pl:91,92,112,129,142,143`, `scopes.pl:137,139`,
`occurrence_identity.pl:83,101,141,152`, `state_machine.pl:44,85`,
`operators.pl:28`.

Every one is `Next := SoFar <op> Piece` under a `pre(...)` read (or, at
`operators.pl:28`, a self-carry `pulse(SoFar)` trigger). This is the register
idiom ARCH.pl calls out as "THE REGISTER ROW IS pre", and it is rx `scan`.

### (c) naming-for-reuse, 1 site

`expressions.pl:165`: `Sum := Base + Extra` feeding both the head **and** the
guard `Sum > 10`. The only site in the corpus where the bound name is read
twice, and therefore the only site where dropping `:=` costs textual
repetition.

### (d) 0 sites

No site puts a `:=`-bound variable into a body **atom** argument. That is the
one position where substitution would change meaning (section 3), and the
corpus never goes there.

---

## 2. Composition proof

Two graded legs, because the compiler and the oracle refuse different things.

### 2a. Compiled leg — emitted modules, real `.dl6` text door (`pairs.sh`)

Byte identity of the emitted module grades **both emitter modes at once**: one
module carries `insertSql`/`supportSql` (incremental) and `recomputeSql` (naive
snapshot referee) side by side, and `SPREFA_TSV2_EMITTER_MODE` only selects
which the runtime executes.

| pair | `:=` spelling | composed spelling | result |
|---|---|---|---|
| map, arithmetic | `Out := Value * 2` | `doubled(Name, Value * 2)` | **BYTE_IDENTICAL** |
| map, concat | `From := concat([...])` | head-position `concat([...])` | **BYTE_IDENTICAL** |
| map, two expressions | `Quotient := ..., Remainder := ...` | both in head | **BYTE_IDENTICAL** |
| chained binds | `Bumped := V+1, Doubled := Bumped*2` | `scaled(Name, (V+1)*2)` | **BYTE_IDENTICAL** |
| naming-for-reuse | `Sum := B+E, Sum > 10` | `over_budget(N, B+E) <- ..., B+E > 10` | **BYTE_IDENTICAL** |
| json braces value | `Doc := {langs: Langs}` | `lang_doc(Name, {langs: Langs})` | REFUSED_BOTH_SAME (`json_value_expression`) |
| **edge head arithmetic** | `pulse(Next) <+ ..., Next := SoFar+1` | `pulse(SoFar + 1) <+ ...` | **ASYMMETRY** (see 2c) |

`RESULT byte_identical=5 refused_both=1 asymmetric=1`. Sabotage receipt: changing
`* 2` to `* 3` in one leg flips `map_arithmetic` to DIFFERING.

The naming-for-reuse row is the sharpest one. `:=` is **not** a
common-subexpression device: the `:=` spelling already emits the expression
twice, exactly as the repeated spelling does —

```sql
INSERT OR IGNORE INTO "over_budget" ("name", "sum")
SELECT b0."name", (b0."base" + b1."extra")
FROM "seen" b0, "bump" b1
WHERE b1."name" = b0."name" AND ((b0."base" + b1."extra") > 10)
```

— so the cost of dropping `:=` there is source characters, nothing else.

**What `:=` actually buys, measured.** Without a `rel` decl the head column name
comes from the surface variable, so `Out := Value * 2` names the column `out`
while the head-expression spelling falls back to positional `col2`. Add the
decl and the two modules are byte-identical. `:=` is a column-naming
convenience on undeclared rels, and nothing more.

### 2b. Oracle leg — tick logs, `.dl6` text door (`oracle_pairs.sh`)

The `pre`-fed accumulation class is refused by the **compiler** for both
spellings alike (`edge_body_needs_pre`, the `pre_occurrence_loop` arc), so it is
graded where it runs.

| pair | result |
|---|---|
| counter fold (`+1` / `-1`, keyed, multi-arrival tick) | **TICKLOG_IDENTICAL** |
| concat fold over a log driver | **TICKLOG_IDENTICAL** |
| async state machine (retries fold + `gave_up` retraction) | **TICKLOG_IDENTICAL** |
| queue, two heads sharing one `pre` plus `latest()` | **TICKLOG_IDENTICAL** |
| edge head self-carry (`repeat`) | **TICKLOG_IDENTICAL** |

`RESULT ticklog_identical=5 differing=0`. Sabotage receipt: `Total + 1` →
`Total + 2` in one leg flips the counter fold red at tick 2.

### 2c. Whole-corpus expansion grade (`grade_expansion.pl`)

`0_assign_expand.pl` is a prototype desugaring in the `0_enum_expand` /
`0_match_expand` shape: erase every `Variable := Expression` goal and bind the
variable to the expression term, letting prolog's own variable sharing do the
substitution at every remaining occurrence.

**19 of 19 fixtures whose programs carry `:=` produce an identical outcome
written vs expanded** (`identical=19 differing=0`), where "outcome" is the tick
log, or an equal throw for the fixtures that exist to exercise an engine
rejection. Sabotage receipt: binding `Variable = Expression + 1` instead drops
that to `identical=4 differing=15`.

The expansion is 100 lines and has exactly one refusal. Migration cost for the
whole corpus is therefore **mechanical**.

Two traps the expansion hit, both worth keeping:

- **Term-door programs share variables across rules.** A fixture's rule list is
  one prolog term, so `Name`/`Next`/`Total` in rule 1 are the *same cells* as in
  rule 2. Expanding in place bound `Next` globally and left rule 2 reading
  `Total+1 := Total-1`, silently deleting the whole decrement arm. `copy_term`
  per rule matches the text door, where `parse_dl` scopes variables per rule.
  `0_enum_expand` and `0_match_expand` never hit this because neither BINDS a
  variable. Any future expansion that binds must copy per rule.
- **The grader must copy before expanding**, or the written-spelling leg silently
  grades the already-expanded term.

---

## 3. Evaluable vs constructor: `foo(bar + 1)` is data or computation?

Stated precisely, from `lower.pl:compile_expr/4` (the header calls itself "the
ONE expression compiler, used for head arguments, `:=` right-hand sides,
comparison operands and aggregate arguments alike") and its mirror
`body.pl:eval_expr/2`:

> **POSITION decides whether a term is an expression at all. FUNCTOR decides
> whether an expression evaluates. Sub-arguments are always expressions.**
>
> 1. **Expression positions**: head arguments, `:=` / `is` right-hand sides,
>    comparison operands, aggregate arguments. **Pattern positions**: every body
>    ATOM argument (a plain rel atom, `pre`, `latest`, `not`, `finalize`, match
>    arms). A pattern position never computes; it destructures.
> 2. In an expression position, a compound term EVALUATES if its functor/arity
>    is in the evaluable inventory, and is a CONSTRUCTOR otherwise.
> 3. A constructor's sub-arguments are recursively compiled as expressions.
> 4. Anything else is a named refusal.

**The evaluable inventory, measured.** `registry.pl`'s `expression/5` table has
**11 rows, not 13**: 5 arithmetic (`+ - * / mod`), 4 ordered comparisons, 2
identity comparisons. Only the 5 arithmetic rows are reachable in argument
position; the 6 comparisons are guard goals and never nest inside a value.

The table is **not** the complete authority, and the brief's premise that it is
should be corrected. `compile_expr` additionally evaluates `concat/1`, and
treats `{}/1` braces and lists as a third category (`json_value_expr`, a named
refusal on the compiled side, `json_canon` on the oracle side). Those three
forms are evaluable in the oracle and absent from `expression/5`. If the
registry is to be the authority the ruling implies, `concat/1` and the json
value forms owe it rows.

**Worked receipt.** `wrapped(Name, pair(Value + 1)) <- reading(Name, Value)`
compiles to

```sql
SELECT b0."name", json_object('fn', 'pair', 'args', json_array((b0."value" + 1)))
```

`pair` is data; its argument is computation. Rendered through the module's own
boundary read against real sqlite3: `pair(42)` for `Value = 41`.

The same text in a body atom is a pattern, not a computation:
`over_ten(Base + 1)` reaches
`join_column_type_mismatch('json_extract(b1."value", '$.args[0]')', text, 'b0."base"', int)`
— it destructured a stored compound and tried to match `args[0]`. Never
arithmetic.

### FINDING: a live oracle-vs-emitter divergence on constructor sub-arguments

The two sides disagree on rule 3. `compile_expr` recurses into a constructor's
sub-arguments; `body.pl:eval_expr/2`'s final clause `eval_expr(Value, Value)`
returns the whole term unevaluated. Graded on the same program and schedule:

| side | `wrapped` row for `Value = 41` |
|---|---|
| oracle (`dl6_oracle.pl`) | `pair(+(41,1))` |
| emitter (real sqlite3 on the module's own boundary SQL) | `pair(42)` |

Zero fixture coverage; the corpus never nests an expression inside a
constructor. Same class as review-A4 (a divergence kept green by absent
coverage). **Unowned defect** — needs a ruling on which side is right (the
emitter reading is the one consistent with rule 3 as written) and a fail-first
fixture either way.

### FINDING: `head_arithmetic` on edge heads is a stale refusal

`analyze.pl:1131` refuses arithmetic anywhere in an EDGE head. Its own comment
(`analyze.pl:1127-1130`, and the block at :1285) says it is defensive and holds
"until real arithmetic lowering lands" — which landed for level heads in the
expression-lift arc. Result: `pulse(Next) <+ pulse(SoFar), SoFar < 3, Next :=
SoFar + 1` compiles, while the identical-meaning `pulse(SoFar + 1) <+
pulse(SoFar), SoFar < 3` refuses, and the oracle grades the two **byte-identical**.
This is the only measured position where composition fails, and it fails for a
reason that is no longer true. Lifting it is the one code change any of the
three cards below needs.

---

## 4. rx lowering for every spelling shown

User law: a construct whose rx lowering cannot be written is a design defect.
The governing sentence is the user's own: **a rel head write IS `next`; `:=` is
a local `const` inside an operator callback and is never an emission.**

**(a) map.** `doubled(Name, Value * 2) <- reading(Name, Value), Value >= 10`

```ts
reading$.pipe(
  filter(({ value }) => value >= 10),
  map(({ name, value }) => ({ name, out: value * 2 })),
);
```

The `:=` spelling is the same pipeline with a named local:

```ts
map(({ name, value }) => { const out = value * 2; return { name, out }; })
```

Identical emission either way, which is the receipt in section 2a restated.

**(b) scan.** `counter(Name, Total + 1) <+ increment(Name, _), pre(counter(Name, Total))`

```ts
increment$.pipe(
  groupBy(({ name }) => name),
  mergeMap(perName$ => perName$.pipe(
    scan((total) => total + 1, 0),
    map(total => ({ name: perName$.key, total })),
  )),
);
```

`pre(...)` is `scan`'s accumulator argument. The register idiom is the seed.

**(c) naming-for-reuse.** `over_budget(Name, Base + Extra) <- seen(...), bump(...), Base + Extra > 10`

```ts
combineLatest([seen$, bump$]).pipe(
  filter(([seen, bump]) => seen.name === bump.name),
  map(([seen, bump]) => ({ name: seen.name, sum: seen.base + bump.extra })),
  filter(({ sum }) => sum > 10),
);
```

Note the rx version names `sum` once and reads it twice, because `map` and
`filter` are two callbacks and the value crosses between them as a field. That
is the honest argument FOR keeping a shorthand: the rx pipeline itself does not
repeat the expression. The emitted SQL does repeat it, in both spellings.

**(d) chained binds.** `scaled(Name, (Value + 1) * 2)` is one `map` whose body is
one expression; the two-`:=` spelling is one `map` with two `const`s. Same
operator, same emission.

**slice.** Not this lab's leg. Per plans/2026-07-29-file-span-design.md,
`slice(span, rel_start, rel_end)` is sub-range projection over a `file_span`,
not destructuring; its rx lowering belongs to that arc.

---

## 5. RULING CARDS

The user decides; this lab prices. All three cards assume the
`head_arithmetic` lift from section 3, which is the same one-line change in
every case.

### Card 1 — surface spelling

| option | what it means | migration cost | what it costs the author |
|---|---|---|---|
| **1A. status quo** | `:=` stays a first-class bind goal; argument-position expressions also stay | zero files | two spellings for one thing, permanently. The construct budget pays for a shorthand that was never ruled. |
| **1B. `:=` is sugar, expansion in the shared pass** | one `0_assign_expand.pl` in `v6/prolog/`, consumed by oracle and compiler like enum/match; downstream sees only argument-position expressions | 1 new file (~100 lines, written and graded); 1 line in `1_expansion.pl`'s declared phase order; `registry.pl` `':='/2` row moves to a sugar rendering; zero fixture edits, zero `.dl6` edits | nothing. Both spellings keep working. |
| **1C. `:=` dies** | the goal is removed; a named refusal points at argument position | as 1B, plus rewriting **30 use sites** across 7 fixture files and 1 `.dl6` — of which 7 dissolve on their own under `file_span` and 15 are one-line mechanical `Next := X` → head `X`. **Mechanical, graded 19/19 identical.** | the naming-for-reuse site (1 in the corpus) repeats its expression; long head expressions lose their name (`flagship-flow`'s concats would have been the ugly case, and they dissolve) |

Phase-order note for 1B/1C: assign expansion must run **after** match arms are
expanded (arms carry bodies) and can run before or after enum; the forced order
today is enum → decl spread → row spread → match, so assign appends.

The `match`/`scan`/`slice` framing the user asked for survives every option:
`match` is the arm block, `scan` is `pre`, `slice` is the file_span leg. `:=`
was never one of the three; it is a naming device orthogonal to all of them.

### Card 2 — SLOT-EXPR-IN-ARG-POSITION

Where may an expression appear? Today's behaviour is inconsistent, so this
needs a word regardless of card 1.

| position | today | options |
|---|---|---|
| level rule head argument | **evaluates** | keep |
| edge rule head argument | **refused** (`head_arithmetic`, stale) | **(i) lift to match level heads** (recommended; the oracle already grades it identical) / (ii) keep refusing and then card 1C is impossible for class (b) |
| `:=` / `is` right-hand side | evaluates | keep |
| comparison operand | evaluates | keep |
| aggregate argument | evaluates | keep |
| **body atom argument** | **pattern** (destructures; silently, via the struct plane) | **(i) stays a pattern forever, and the expansion refuses substitution there by name** (recommended; matches prolog, and the corpus never wants otherwise) / (ii) evaluates, which makes `over_ten(Base+1)` a computation and costs the destructure spelling |
| constructor sub-argument | **compiler evaluates, oracle does not** | must be ruled; see the divergence finding |

### Card 3 — what `=` does

Today `=` is nobody's construct: it has **no `registry.pl` row at all**, so
refusal-by-absence should name it. It does not. Measured:

| door | program | what you get |
|---|---|---|
| compiler | `doubled(Name, Out) <- reading(Name, Value), Out = Value * 2.` | `unsupported_construct(unbound_head_var(_))` |
| compiler | `flagged(Name) <- reading(Name, Value), Out = Value * 2.` (head has **no** unbound var) | still `unbound_head_var(_)` — the refusal is not merely misnamed, it names a location that does not exist |
| oracle | same program | `unbound_in_expression`, printed as swipl `Unknown message` with no file or line (review-B4, third sighting) |

Options:

- **3A (recommended).** Give `=` a `registry.pl` row as a refused surface with a
  message that points at the chosen spelling: under 1A/1B, "use `:=`"; under 1C,
  "put the expression in argument position". Cost: one registry row, one
  `0_refusal_messages.pl` clause, one refusal fixture. This is the smallest
  correct fix and it closes the wrong-location bug as a side effect.
- **3B.** Make `=` an alias for the bind goal. Reopens the mode-polymorphism the
  aggregate analysis already killed (`=` would have to be both bind and compare);
  not recommended, and the kill has a written receipt.
- **3C.** Leave it. Every cold author who types `=` gets a refusal that names the
  wrong thing in the wrong place.

`is/2` is a live registry row and a silent second spelling of `:=` used by zero
corpus sites. Whatever card 1 chooses, `is` should follow `:=` rather than
outlive it.

---

## 6. Lab death

Lab files at `v6/prolog/labs/assign_composition/` (probes, `0_assign_expand.pl`,
`grade_expansion.pl`, `grade_emitted.pl`, `guard_check.pl`, `pairs.sh`,
`oracle_pairs.sh`, `oracle.sh`, `probe.sh`, `show_one.pl`) are deleted in the
death commit recorded below. Recover with `git show <hash>:<path>`.

**Last copy: `4c2255a3`**

`0_assign_expand.pl` is the one file worth resurrecting: under card 1B or 1C it
moves to `v6/prolog/` unchanged in shape.
