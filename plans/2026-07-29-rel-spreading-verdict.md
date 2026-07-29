# REL SPREADING VERDICT (lab, 2026-07-29)

Contract: `plans/2026-07-29-rel-spreading-lab-header.md`.
Lab: `v6/prolog/labs/rel_spreading/` (dies on landing; the recovery hash is at
the bottom of this file).
Cross-language receipts: `v6/prolog/labs/rel_spreading/RECEIPTS.md`, regenerated
by `bash probes/run_probes.sh`, every block verbatim compiler stdout+stderr.

Run from `v6/prolog`:

```text
swipl -q -l labs/rel_spreading/lab.pl -g go -g halt
```

54 PASS, exit 0, PASS-only stdout, byte-identical across two runs.

---

## VERDICT LINE

**Spreading is a DECLARATION-TIME COLUMN SPLICE, expressible today as one more
expand-module in the `0_enum_expand.pl` / `0_match_expand.pl` chain, with zero
knowledge downstream of the expansion.** The graded proof is
`c1_expanded_program_is_the_hand_written_program`: after expansion the spread
program is `=@=` the hand-written program, decl entry for decl entry, and
`c1_engine_gives_both_programs_the_same_tick_log` runs both through the oracle
engine to the same deltas.

Three things it costs, all found by writing it rather than assumed:

1. **A spread rel's arity stops being syntactic.** The sugar term cannot carry
   `Name/Arity` the way every other decl entry does, because the arity is what
   the splice computes (`c1_spliced_arity_is_computed_not_written`). Modifier
   entries therefore accept a bare `Name` and get the arity filled in; a
   written `Name/Arity` is checked, never trusted
   (`c1_written_arity_that_disagrees_is_refused`).
2. **The splice draws a hard line between GENERATED and INFERRED columns.** An
   enum variant rel is a legal spread source because enum expansion emits real
   `col_type` entries (`c6_an_enum_variant_rel_is_a_legal_spread_source`); a
   derived rel is not, because its columns only exist after
   `analyze.pl:rel_columns/4` runs with the surface variable bindings, and the
   expansion runs before that (`compile.pl:92`, `engine.pl:349`). Case C6 is
   confirmed blocked, not designed around.
3. **`key(...)` positions on a spread rel are computed against the spliced
   order**, so widening the source silently moves them. Named slot below.

Cross-language ground truth agrees on the shape and disagrees on the
collision and inheritance rules. TypeScript has a positional splice
(variadic tuple types, RECEIPTS.md C1 `tsc` exit 0) and resolves name
collisions silently by last-wins (RECEIPTS.md C2 exit 0, no diagnostic).
Rust has no field splice at all (`error: expected identifier, found ..`) and
its only spread-shaped form, struct update, requires the same type
(`E0308 expected BRow, found ARow`). Go's inclusion mechanism promotes rather
than splices (`cannot use 3 (untyped int constant) as ARow value in struct
literal`), accepts a colliding declaration silently, and reports it only at a
use site (`ambiguous selector m.Shared`).

---

## Per-case verdict table

| case | draft call | model check result | ts receipt | rust receipt | go receipt | selected semantics |
|---|---|---|---|---|---|---|
| C1 decl spread | compile-time column splice | HOLDS, 11 checks | C1: variadic tuple splice accepted, exit 0; width negative `TS2322 Source has 2 element(s) but target requires 3`; no type-level object spread `TS1131` | C1: `error: expected identifier, found \`..\`` on the decl; `E0308` on cross-type functional update | C1: `too many values in struct literal of type BRow` at the spliced-width literal | one expand-module, positional splice at the written point, source order preserved, expansion output is ordinary `col_type` entries |
| C2 collision | named refusal | HOLDS, 5 checks | C2: exit 0, `shared` silently takes the last source's type, and flipping the order flips the type with no diagnostic; duplicate tuple LABELS also accepted | C2: `E0124 field \`shared\` is already declared`, `E0062 field \`shared\` specified more than once`, and two base structs refused outright | C2: declaration accepted, `ambiguous selector m.Shared` only at the use site | named refusal `spread_column_collision(Rel, Column)` at expansion, regardless of whether the types agree |
| C3 row spread in a head | one fresh variable per spliced column, head arity total | HOLDS, 10 checks | C3: spread followed by explicit arguments accepted for a TUPLE, refused for an array `TS2556 A spread argument must either have a tuple type or be passed to a rest parameter` | C3: no form; `E0061 this function takes 3 arguments but 2 arguments were supplied`, the `..a` parsed as a `RangeTo` | C3: `syntax error: unexpected literal 5, expected )` -- Go forbids a spread followed by explicit arguments | marker binds N fresh variables shared across every occurrence; width read from the DECLARED arity of the atom it is spread into; head must be total after the splice |
| C4 width subtyping | REFUSE, rels stay nominal | HOLDS, 4 checks | C4: the wider value through a binding is ACCEPTED (structural, no diagnostic); only the fresh literal is caught `TS2353`; positional rows are exact `TS2345 Source has 2 element(s) but target allows only 1` | C4: `E0308 expected Narrow, found Wide`, and a structurally identical second struct is also refused | C4: `cannot use w (variable of struct type Wide) as Narrow value` | refuse by width: `head_arity_mismatch(Name, Spliced, Declared)`. An equal-width cross-rel splice is accepted and grants nothing positional variables did not already grant |
| C5 plane and key inheritance | columns only, never key/keep/log | HOLDS, 6 checks | C5: value spread DROPS `readonly` (no diagnostic on the write), carries optionality, tuple splice drops `readonly`; only intersection keeps it `TS2540` | C5: `E0277 the trait bound TargetRow: Copy is not satisfied` -- derives do not ride the field copy | C5: the EMBEDDING struct silently satisfies `Keyed`; only the hand-copied one is refused `CRow does not implement Keyed (missing method Key)` | columns only. The source's `kind`, `keyed` and `keep` stay with the source |
| C6 derived spread source | blocked on the type pass | CONFIRMED BLOCKED, 6 checks | C6: derived and forward references BOTH accepted (full type pass first), circular refused `TS2456 Type alias 'SelfRow' circularly references itself` | C6: no splice syntax, and computed width needs `#![feature(generic_const_exprs)]` -- `error: generic parameters may not be used in const operations` | C6: forward reference accepted, `invalid recursive type: SelfRow refers to itself` | named refusal `spread_source_not_declared(Name)`. Generated decls (enum) are legal sources; inferred ones are not |
| C7 host decl spread | falls out of C1 | HOLDS, 7 checks | C7: `...args: [...CommonInputs, endpoint: string]` accepted; width enforced at the call `TS2554 Expected 3 arguments, but got 2`; the OUTPUT side has no positional splice, only intersection | C7: `error: expected one of ::, :, or |, found ,` | C7: `syntax error: ... is missing type` | the same resolver over two column lists. Each side splices independently, so the input/output split survives, and the existing template refusals fire on spliced columns unchanged |
| C8 rest beyond kwargs | out of scope | BOUNDARY RECORDED, 2 checks | n/a | n/a | n/a | the row-spread expansion requires a TOTAL head, so an omitted-column head is refused rather than silently absorbed. Partial application belongs to the kwargs lane (`parse_dl.pl:590 fill_free_slots`) |

---

## Case detail

### C1 declaration spread

Sugar term:

```prolog
spread_decl(b, [spread(a), col(extra, int)])
```

Expansion output, in place, source order preserved:

```prolog
col_type(b/3, id, int)
col_type(b/3, name, text)
col_type(b/3, extra, int)
```

| criterion | result | check |
|---|---|---|
| spliced order | `[id, name, extra]`, source order of the source's own decl entries | `c1_splice_produces_columns_in_source_order` |
| types | carried from the source unchanged | `c1_splice_carries_the_source_column_types` |
| downstream knowledge required | zero; expanded program is `=@=` hand-written | `c1_expanded_program_is_the_hand_written_program` |
| runtime behavior | identical deltas through the oracle engine | `c1_engine_gives_both_programs_the_same_tick_log` |
| arity | computed, absent from the sugar term | `c1_spliced_arity_is_computed_not_written` |
| bare modifier ref | resolved to the computed arity | `c1_bare_modifier_ref_takes_the_computed_arity` |
| written modifier arity that disagrees | `spread_arity_conflict(b, 2, 3)` | `c1_written_arity_that_disagrees_is_refused` |
| nested spread source | resolves, `[id, name, extra, tail]` | `c1_nested_spread_source_resolves` |
| cycle | `spread_cycle([a, b, a])` | `c1_spread_cycle_is_refused` |
| unknown source | `spread_source_not_declared(missing)` | `c1_unknown_spread_source_is_refused` |
| spread rel that also declares its own columns | `spread_and_explicit_columns(b)` | `c1_spread_rel_may_not_also_declare_its_own_columns` |

Rust's receipt is the counterweight worth naming: the language that is closest
to this engine's storage model has no field splice at all, and its one
spread-shaped form is value-level and same-type-only. That does not argue
against the construct; it argues that the splice must be a compile-time
rewrite with no runtime residue, which is exactly what the expansion is.

### C2 column collision

Refusal: `spread_column_collision(Rel, Column)`, thrown at expansion, before
any arity is computed. Fires for two spread sources sharing a name
(`c2_column_collision_is_refused`), for two sources whose types also agree
(`c2_collision_is_refused_even_when_the_types_agree`), and for a spread source
colliding with an explicitly written column
(`c2_a_collision_with_an_explicit_column_is_refused`).

The TypeScript alternative is modelled rather than described. `last_wins_columns/2`
in `lab.pl` reproduces the object-spread rule, and the graded consequence is:

| input | last-wins result | count |
|---|---|---|
| `[shared:int, only_a:text, shared:text, only_x:int]` | `[only_a:text, shared:text, only_x:int]` | 4 columns in, 3 out |
| `[shared:int, shared:text]` vs `[shared:text, shared:int]` | different results | order-dependent |

Checks `c2_last_wins_would_have_dropped_a_column_silently` and
`c2_last_wins_result_depends_on_source_order`. In a positional language a
dropped column is a changed arity, so every rule written against the rel shifts.
That is the whole argument for the refusal, and it is stronger here than in
TypeScript because TypeScript's rows are named.

### C3 row spread in a head

Surface and term:

```dl
c(...a_row, 5) <- a(...a_row).
```

```prolog
(c(spread(Marker), 5) <- a(spread(Marker)))
```

What the marker binds: **one fresh variable per spliced column, shared across
every occurrence**. The splice is therefore an ordinary positional join and the
expanded rule is `=@=` the hand-written positional rule
(`c3_spliced_rule_equals_the_hand_written_positional_rule`).

| question | answer | check |
|---|---|---|
| where does the width come from | the DECLARED arity of the relation the marker is spread into, minus the explicit slots written beside it | `c3_marker_binds_one_fresh_variable_per_spliced_column` |
| do head and body share the variables | yes, by identity | `c3_splice_shares_the_same_variables_between_head_and_body` |
| explicit slots after the marker | allowed | `c3_spliced_rule_equals_the_hand_written_positional_rule` |
| explicit slots before the marker | allowed | `c3_explicit_slots_may_precede_the_marker` |
| head totality | enforced, `head_arity_mismatch(c, 2, 3)` | `c3_head_arity_totality_is_enforced` |
| marker only in the head | `row_spread_unbound_in_head` | `c3_marker_that_never_reaches_a_body_atom_is_refused` |
| two body widths for one marker | `row_spread_width_conflict(2, 3)` | `c3_two_body_widths_for_one_marker_are_refused` |
| two markers in one atom | `multiple_row_spreads_in_atom(Atom)` | `c3_two_markers_in_one_atom_are_refused` |
| body atom with no declared columns | `row_spread_width_unknown(Name)` | `c3_a_body_width_source_with_no_declared_columns_is_refused` |
| runs | oracle engine emits `[+c(1,"x",5)]` | `c3_engine_runs_the_spliced_rule` |

Go is the one language that forbids "spread then explicit argument" outright
(RECEIPTS.md C3, `syntax error: unexpected literal 5, expected )`), because its
spread is a variadic-call form and must be terminal. TypeScript allows it
because variadic tuples are fixed-width types. dl rows are fixed-width by
declaration, so the TypeScript rule is the applicable one, and both leading and
trailing explicit slots are accepted here. Slot below on whether to keep both.

The width source is a real dependency: row spread requires the DECL spread to
have already run, so the expansion order is enum, then decl spread, then row
spread, then match.

### C4 width subtyping

The check that catches it is arity, and it catches both directions:

| written | refusal | check |
|---|---|---|
| `narrow(...wide_row) <- wide(...wide_row)` | `head_arity_mismatch(narrow, 3, 2)` | `c4_wider_row_into_a_narrower_head_is_refused` |
| `wide(...narrow_row) <- narrow(...narrow_row)` | `head_arity_mismatch(wide, 2, 3)` | `c4_narrower_row_into_a_wider_head_is_refused` |
| any width mismatch | expansion produces no result at all, never a truncated or padded one | `c4_the_expansion_never_truncates_or_pads` |

The honest half: an EQUAL-width splice across two different rels is accepted
(`c4_equal_width_cross_rel_splice_is_positional_not_subtyping`). Nominal
identity comes from the functor, not from the splice; the accepted case is
byte-identical to writing the variables out. Spread grants no coercion that
positional variables did not already grant.

TypeScript's receipt is the reason this matters. `takesNarrow(wide)` through a
binding is ACCEPTED with no diagnostic (RECEIPTS.md C4, the only object-shape
error is on the fresh literal). Structural width subtyping is exactly the
behavior the nominal call refuses, and TypeScript itself falls back to exact
width checking as soon as the rows are positional
(`TS2345 Source has 2 element(s) but target allows only 1`).

### C5 plane and key inheritance

Selected: the splice carries COLUMNS ONLY.

| entry on the source | crosses the splice | check |
|---|---|---|
| `col_type` | yes | `c5_spread_carries_columns_only` |
| `keyed` | no | `c5_the_source_key_does_not_cross_the_splice` |
| `kind` | no | `c5_the_source_kind_and_retention_do_not_cross_the_splice` |
| `keep` | no | same |

The negative is graded by running the inheriting variant through the real
engine, not by argument:

| inherited word | consequence | check |
|---|---|---|
| `keyed` | a program the engine loads becomes one it refuses: `keyed_level_head(b/3)` (`engine.pl:104`) | `c5_inheriting_the_key_turns_a_loadable_program_into_a_refusal` |
| `keep` | final state silently shortens: 3 arrivals leave 3 rows selected, 1 row inheriting | `c5_inheriting_the_retention_silently_shortens_the_log` |
| `keep`, graded again | on 2 arrivals the two programs emit BYTE-IDENTICAL tick logs (no retraction delta on either side) while their final states differ, 2 rows against 1 | `c5_the_retention_inheritance_is_invisible_in_the_tick_log` |

That last row is the same class as the standing retention-grading gap the
sweep's final-state leg exists for: a retention change is invisible to
tick-log-only grading. A spread that imported `keep` would therefore be a
behavior change no delta diff could see.

Go is the graded negative in another language: embedding promotes methods and
interface satisfaction along with the fields, so `BRow` silently satisfies
`Keyed` while the hand-copied `CRow` does not (RECEIPTS.md C5, `go build`
refuses only `CRow`). Rust is the graded positive: derives do not ride a field
copy (`E0277 the trait bound TargetRow: Copy is not satisfied`). TypeScript
splits: value spread drops `readonly` but carries optionality; only
intersection keeps `readonly`. The selected semantics matches Rust and the
`readonly`-dropping half of TypeScript, and deliberately does not match Go.

### C6 spreading a derived rel

Confirmed blocked. The failure shape today:

```prolog
prog([ spread_decl(b, [spread(pair), col(extra, int)]) ],
     [ (pair(Id, Tag) <- source(Id, Tag)) ])
```

throws `unsupported_construct(spread_source_not_declared(pair))`
(`c6_a_derived_rel_is_not_a_legal_spread_source`).

What inference hands you, once the phase that owns it runs
(`compile/analyze.pl:rel_columns/4`, called from the lab against the real
module):

| program | inferred columns | check |
|---|---|---|
| `pair(Id, Tag) <- source(Id, Tag)` | `[id, tag]`, from the surface variable names | `c6_inference_recovers_columns_only_from_rules_and_bindings` |
| `pair(_, 'lit') <- source(_, _)` | `[col1, col2]`, the positional fallback | `c6_inference_falls_back_to_positional_names` |
| either | names only, no types; types are a separate program-wide fixpoint over literal witnesses | `c6_inference_gives_no_types_for_a_derived_rel` |

Two inputs that do not exist where the expansion runs: `Rules` (the expansion
sees `prog/2` but has no reason to walk it) and `Bindings`, the surface
variable names the caller recovers via
`read_term(..., [variable_names(Bindings)])`. A program that arrived as a term
rather than as text has no `Bindings` at all, so a derived spread source would
splice `col1, col2` in the term door and `id, tag` in the text door: the same
program with two different arities-worth of column names depending on which
door it came through.

The line the lab actually draws is GENERATED versus INFERRED, not derived
versus declared:

| source | legal | check |
|---|---|---|
| plain declared rel | yes | C1 checks |
| enum variant rel, after enum expansion | yes, `[col(id,int), col(view,text)]` | `c6_an_enum_variant_rel_is_a_legal_spread_source` |
| the same enum variant rel, if spread runs FIRST | no, `spread_source_not_declared(body_page)` | `c6_the_same_enum_source_is_refused_if_spread_runs_first` |
| derived rel | no | `c6_a_derived_rel_is_not_a_legal_spread_source` |

TypeScript is the counterexample that shows what a full type pass buys:
`type SplicedFromDerived = [...ReturnType<typeof derive>, extra: number]` is
accepted, and so is a forward reference, because the type pass is neither
source-ordered nor rule-blind (RECEIPTS.md C6). It still refuses the circular
case, and so do Rust and Go, so cycle refusal is universal even where derived
sources are legal.

### C7 spread in host declarations

It falls out of C1 exactly as the contract guessed, and the receipt for that
is that the host side reuses `spread_columns/3` unchanged
(`c7_the_host_side_needs_no_spread_code_of_its_own`).

```dl
sh fetch(...common, ep: text) -> (...common_out) = `get {repo} {rev} {ep}`.
```

```prolog
sh_decl(fetch,
        [spread(common), col(ep, text)],
        [spread(common_out)],
        template("get {repo} {rev} {ep}"))
```

| criterion | result | check |
|---|---|---|
| input side splices | `[repo, rev, ep]` | `c7_host_input_spread_splices_through_the_same_resolver` |
| output side splices independently | `[status, tag, body]` | `c7_host_output_spread_splices_independently` |
| the explicit input/output split survives | yes, compiles to `host_plan/4` | `c7_the_input_output_split_survives_the_splice` |
| spliced input absent from the template | `template_mismatch(unreferenced_input(repo))` | `c7_a_spliced_input_absent_from_the_template_is_refused` |
| spliced output referenced by the template | `template_mismatch(output_used_as_input(status))` | `c7_a_spliced_output_referenced_by_the_template_is_refused` |
| spliced name colliding across the two sides | `column_mismatch(input_output_overlap(ep))` | `c7_a_name_colliding_across_the_two_sides_is_refused` |

Every one of those refusal names is the one the hosts+extraction verdict
recorded. A spliced column and a hand-written column produce the same refusal,
which is the property that makes the composition free.

The hosts verdict selected the explicit input/output split so that a template
edit cannot silently flip a column's mode. The splice does not weaken that:
each side is resolved by a separate call, so a spread on the input side can
never move a column to the output side. TypeScript's receipt shows the shape a
language reaches without that split: the input side gets a positional splice
and the output side has none, so the two sides need different spellings
(RECEIPTS.md C7, `CommonOutputs & { body: string }`).

### C8 rest and partial application

Out of scope by the contract, boundary recorded executably. The row-spread
expansion requires a TOTAL head after the splice, so a head written with fewer
columns than declared is refused `head_arity_mismatch(c, 1, 3)` rather than
silently wildcard-filled (`c8_an_omitted_column_head_is_refused_not_absorbed`),
and a rule with no spread marker passes through unchanged
(`c8_this_lab_adds_no_wildcard_fill`).

Partial application (body atoms may omit columns, each omission becoming a
fresh wildcard, heads staying total) is the concurrent kwargs lane's task; its
current exact-fill gate is `parse_dl.pl:590 fill_free_slots`. The two
constructs meet at one place only: a body atom containing BOTH a spread marker
and an omitted column. Named slot below.

---

## Spelling pricing

Three candidates, criteria visible, no fiat.

### A. `...name` inside the column list

```dl
rel b(...a, extra: int).
```

### B. an `include` decl clause

```dl
rel b(extra: int) include a.
```

### C. term form only, no `.dl6` surface

```prolog
spread_decl(b, [spread(a), col(extra, int)])
```

| criterion | A `...name` | B `include` | C term only |
|---|---|---|---|
| can state WHERE the source columns land | yes, the marker's position | **no** | yes |
| leading vs trailing splice distinguishable | yes, `[id,name,extra]` vs `[extra,id,name]` | **no, both project to the same include form** | yes |
| honors `decl_column_spelling = colon_typed_ordered_columns` (source order significant) | yes | violated | yes |
| new grammar in `parse_dl.pl` | one marker form inside the existing column list | one trailing clause | none |
| new registry row | 1 | 1 | 0 |
| `print_dl` round-trip (G1 is a position-exact `=@=` over the decl LIST) | prints the marker at its position | cannot reproduce the order it never captured | no text to print |
| `.dl6` text door | works | works | **refuses: no surface** |
| arity visible in source | no (structural cost, all three share it) | no | no |
| downstream phases that must learn anything | 0 | 0 | 0 |

`pricing_include_clause_cannot_state_the_splice_point` is the executable form
of the decisive row: the two spread specs `[spread(a), col(extra,int)]` and
`[col(extra,int), spread(a)]` expand to `[id, name, extra]` and
`[extra, id, name]` respectively, two different positional programs, while both
project to the identical include-form pair `[a]-[col(extra,int)]`. B cannot
distinguish them, so B is not a spelling of this construct at all; it is a
spelling of a weaker one.

`pricing_term_form_only_has_no_registry_row_and_no_text_door` confirms against
the real `registry.pl` that neither `spread_decl/2` nor `spread/1` has a
surface row today, so C is the status quo, and C's cost is that the text door
(`compile_dl6`) refuses every program using the construct.

**Selected: A.** Cost is one registry row and one marker form inside the
existing column list; everything downstream sees ordinary `col_type` entries.
`pricing_the_expansion_precedent_carries_a_live_registry_row` checks that this
is precisely the shape `enum_decl/2` and `match/2` already have: a live
registry row, and no phase past their expansion knows they exist.

### The rx lowering row (house law)

The house law is that every spelling shown carries its pure-rxjs lowering. For
spread the lowering is trivial, and saying exactly why is the point:

| id | spelling | rx lowering |
|---|---|---|
| RX-S1 | `rel b(...a, extra: int).` | **identical to `rel b(id: int, name: text, extra: int).`** The splice completes at expansion time, before any lowering runs, so no operator, no subscription and no runtime value corresponds to it |
| RX-S2 | `c(...a_row, 5) <- a(...a_row).` | identical to `c(id, name, 5) <- a(id, name).`, which is `a$.pipe(map(([id, name]) => [id, name, 5]))` |
| RX-S3 | `sh fetch(...common, ep: text) -> (...)` | identical to the hand-written `sh_decl/4`, lowering RX-H1 in the hosts verdict, unchanged |

Worked example for RX-S2, the spread source and its lowering side by side:

```dl
rel a(id: int, name: text).
rel b(...a, extra: int) log keep(all).

b(...a_row, 7) <+ a(...a_row).
```

```ts
// the ONLY rx this program produces. The splice is not in it.
const b$ = a$.pipe(
  map(({ id, name }) => ({ id, name, extra: 7 })),
);
```

The claim that this is the lowering is not asserted; it is
`c1_expanded_program_is_the_hand_written_program` (`=@=` over the whole
program term) plus `c1_engine_gives_both_programs_the_same_tick_log` (identical
deltas through the oracle engine). A construct whose rx lowering cannot be
written is a design defect; this construct's rx lowering is the lowering of the
program it expands to, and there is nothing left over.

---

## Named slots

| slot | question | why the lab did not answer it |
|---|---|---|
| `slot_spread_key_positions` | `key(...)` on a spread rel names POSITIONS, and the spliced order shifts when the source widens. Keep positions and accept the fragility, or add a by-name key form for spread rels | changing `keyed/2` to accept column names is a language change beyond this contract, and it collides with the standing Q8/Key ruling thread |
| `slot_spread_expansion_order` | the required order is enum, then decl spread, then row spread, then match. Confirmed by `c6_the_same_enum_source_is_refused_if_spread_runs_first`, but the shipped chain is match wrapping enum (`0_match_expand.pl:20`), so inserting two phases is a compile.pl and engine.pl edit both owned by another lane | fence |
| `slot_spread_derived_source` | whether a derived rel ever becomes a legal spread source. It needs the column pass to run before the expansion, and the two doors disagree on what that pass returns (`[id, tag]` from text, `[col1, col2]` from a term) | the type-pass dependency is real, C6 says document not design |
| `slot_spread_marker_position` | leading explicit slots (`d(5, ...a_row)`) are accepted here. Go forbids a non-terminal spread outright; TypeScript allows it. Restrict to trailing-only, or keep both | no criterion in the lab separated them; both expand identically |
| `slot_spread_and_kwargs_overlap` | a body atom containing BOTH a spread marker and an omitted column. Neither lane owns it | C8 is out of scope by the contract; the kwargs lane is concurrent |
| `slot_spread_arity_in_source` | a spread rel's arity is invisible in its own declaration. Whether the printer should emit the computed arity as a comment, or the language should accept that arity is no longer syntactic | presentation question, not semantics |

---

## Refusal inventory

Every refusal the lab implements, all thrown as
`unsupported_construct(What)` at expansion time.

| refusal | fires when |
|---|---|
| `spread_source_not_declared(Name)` | the spread source has no `col_type` entries (unknown rel, or a derived rel) |
| `spread_cycle(Names)` | a spread source chain closes on itself |
| `spread_column_collision(Rel, Column)` | two spliced columns share a name |
| `spread_and_explicit_columns(Name)` | a spread rel also writes its own `col_type` entries |
| `spread_arity_conflict(Name, Written, Spliced)` | a modifier entry writes an arity the splice disagrees with |
| `bare_ref_on_unspread_rel(Name)` | a bare-name modifier ref on a rel that is not spread-declared |
| `spread_spec_shape(Item)` | a column spec item that is neither `spread/1` nor `col/2` |
| `head_arity_mismatch(Name, Spliced, Declared)` | the spliced head does not fill its declared width |
| `head_width_unknown(Name)` | the head rel has no declared columns |
| `row_spread_unbound_in_head` | a row marker never reaches a body atom |
| `row_spread_width_conflict(WidthA, WidthB)` | one marker gets two widths from two body atoms |
| `row_spread_width_unknown(Name)` | a body atom carrying a marker has no declared columns |
| `row_spread_overfills(Name, Written, Declared)` | explicit slots beside the marker already exceed the declared width |
| `multiple_row_spreads_in_atom(Atom)` | more than one marker in one atom |

---

## Verification

| command | receipt |
|---|---|
| `swipl -q -l labs/rel_spreading/lab.pl -g go -g halt` from `v6/prolog` | 54 PASS, exit 0, PASS-only stdout, zero bytes on stderr, run twice with byte-identical output |
| `bash probes/run_probes.sh` from the lab home | `RECEIPTS.md` regenerated; tsc 5.6.3, tsgo 7.0.0-dev.20260707.2, rustc 1.97.0-nightly, go 1.26.3 |
| `swipl -q -l v6/prolog/conformance/go.pl -g go -g halt` | 126 PASS, 0 fail, exit 0 |
| `bash v6/prolog/compile/scripts/roundtrip.sh` | `roundtrip.sh: ALL GRADES PASS`, G1 ALL PASS, G2 NO PARSE ERRORS, G3 126 pass / 0 fail |
| path fence | only `v6/prolog/labs/rel_spreading/*` and this file |
| git posture | committed with `git commit -n`, not merged |

## Lab death

The lab is deleted in the landing commit per the lab protocol. The last full
copy is the commit recorded by the coordinator at merge; recover any file with
`git show <hash>:v6/prolog/labs/rel_spreading/<file>`.
