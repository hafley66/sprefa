# temporal_pipe: verdict on the temporal pipe `|>`

Lab: `v6/prolog/labs/temporal_pipe.pl`, 25 graded checks, all PASS.

```
swipl -q -l v6/prolog/labs/temporal_pipe.pl -g go -g halt      # 25 PASS, exit 0
swipl -q -l v6/prolog/labs/temporal_pipe.pl -g report -g halt  # desugar + traces
```

## Verdict up front

**Conditional yes.** The construct works, the boundary law is implementable and
checkable, binding flow infers cleanly, and the trigger claim is real. Three
conditions, in order of how much they cost:

1. **The glyph `|>` is unlexable in prolog** and so is `!rel`, `x.field`,
   `Entry { .. }`, and `match x { .. }`. Adopting `|>` does not add a
   requirement, it confirms one that four other spec constructs already
   carried: `surface_dcg` is not optional. Nothing about the pipe can be
   prototyped in a prolog-read surface without a stand-in spelling.
2. **The pipe resolves R5 at boundaries only, not inside stages.** Every rule
   after the first gets exactly one trigger atom for free. The FIRST stage of
   every chain still has LANG.md's any-atom trigger over its whole body, and
   the ghcacher chain's first stage joins four atoms. R5 still needs its own
   ruling; the pipe shrinks it, it does not close it.
3. **`|>` cannot state its `sugar/2` fact yet.** It grounds out into `rule` and
   `ground_terms` plus one thing that is not registered: the per-atom trigger
   marker the generated rules carry. Registering the pipe before R5 is ruled
   would put an unregistered primitive into the kernel through the back door,
   which is exactly the registry drift the consolidation flagged.

## 1. Parseability in prolog, measured

### 1a. The glyph does not lex, at any priority

`|` is a SOLO character in the prolog character table, not a symbol character.
Symbol atoms fuse from `+-*/\^<>=~:.?@#&$`; `|` is not in that set, so `|` and
`>` never form one token. `op/3` cannot help: the lab declares
`op(1100, xfy, '|>')` and the unquoted text still fails at the `>`.

| text | result |
|---|---|
| `watch(E) \|> fetch(E)` | `syntax_error(operator_expected)` |
| `watch(E) '\|>' fetch(E)` | reads, principal functor `'\|>'/2` |

Check: `pipe_glyph_needs_quotes`. The lab therefore writes the pipe as `~>`
(all symbol characters) for every other experiment. Priority and associativity,
the properties actually under test, do not depend on the glyph.

### 1b. Working precedence, graded by the shape prolog assigns

`,` is 1000 xfy. `<-` and `<+` are 1150 xfx. Three candidate slots, three
different languages:

| pipe priority | text | shape prolog builds | consequence |
|---|---|---|---|
| 900 xfy | `watch(E), every_300(B) ~> fetch(E)` | `','(watch(E), ~>(every_300(B), fetch(E)))` | pipe binds tighter than comma; stage 1 collapses to its LAST atom; every multi-atom stage needs parentheses |
| **1100 xfy** | `watch(E), every_300(B) ~> fetch(E), keep(B)` | `~>( ','(watch,every_300), ','(fetch,keep) )` | **a stage IS a comma-joined body. This is the working slot.** |
| 1100 xfy under `<+` | `change_log(E) <+ watch(E), every_300(B) ~> fetch(E)` | `<+(change_log(E), ~>(...))` | rule arrow stays principal, whole chain is its body |
| 1150 xfy | `change_log(E) <+ watch(E) ~> fetch(E)` | `syntax_error(operator_clash)` | `<+` is xfx 1150, its right argument must stay under 1150 |
| 1175 xfy | `demand(X) <+ watch(X) ~> folded(X) <+ demand(X)` | `~>( <+(demand,watch), <+(folded,demand) )` | pipe becomes a chain of whole CLAUSES |
| 1175 xfy | `change_log(X) <+ watch(X), every_300(B) ~> fetch(X)` | `~>( <+(change_log, (watch,every_300)), fetch(X) )` | the last stage LOSES its head; a one-head chain no longer parses as one rule |

Checks: `pipe_below_comma_inverts_stages`, `pipe_above_comma_groups_stages`,
`pipe_under_arrow_keeps_one_head`, `pipe_at_arrow_priority_clashes`,
`pipe_above_arrow_becomes_clause_chain`.

**Answer: 1100 xfy.** Strictly between comma (1000) and the rule arrows (1150),
right associative so `S1 ~> S2 ~> S3` nests as `S1 ~> (S2 ~> S3)` and flattens
in one pass. The 1175 reading is a coherent alternative language (a pipe over
whole rules), but it deletes the proposal's own feature: with a head per stage
there is nothing to generate intermediate names for.

### 1c. Break 1, the interaction `Head <+ StageA |> StageB`

At 1100 the rule arrow wins and the result is usable: `<+`/2 with the chain as
its body. The break is not in the parse, it is in what the parse can express.
One arrow token sits in front of N writes. `Head <+ chain` states the storage
kind of the LAST write only; the N-1 intermediate rel writes have no syntax at
all, so the desugarer must pick. This lab picks `<+` for every intermediate
(a cut was crossed, so the intermediate holds occurrences, not membership) and
gives the declared arrow to the final head only. That choice is invisible in
the source text.

At 1150 there is no parse at all, and at 1175 there is a parse that means
something else. The window is one priority band wide.

### 1d. Break 2, dot access `cache(endpoint).tag` inside a stage

Three separate failures, all measured:

| text | what prolog does |
|---|---|
| `out(X) <+ watch(X) ~> cache(X).tag` | reads. `'.'/2` term, SWI dict access syntax |
| `out(X) <+ watch(X) ~> cache(X). tag` | reads as a legal TWO-stage chain, last stage `cache(X)`. `tag` becomes a separate clause |
| `watched(cache(E).tag)` as a fact | `expand_term` rewrites it to `watched(V) :- '.'(cache(E), tag, V)`. The fact is now a rule |
| calling that goal | `type_error(dict, cache(cli))` |

Checks: `dot_access_truncates_on_space`, `dot_access_in_fact_arg_becomes_a_rule`.

The honest reading of the earlier note ("dict dot access does not expand in fact
args") is sharper than stated: it DOES expand, and expanding is the damage. A
fact silently becomes a rule whose body calls `./3`, and `./3` on anything that
is not a dict throws at runtime, not at compile time.

The whitespace case is the one that should decide `surface_dcg`. `. ` is
end-of-clause in prolog. Adding one space to a pipe chain turns a three-stage
chain into a two-stage chain that still type-checks, still desugars, still runs,
and drops a stage. There is no diagnostic anywhere in that path.

The lab itself hit a smaller version: a literal `'.'(A, B)` written in a CHECK
BODY is rewritten by SWI's functional-notation expansion into a `./3` call, so
the lab cannot even quote the shape it is grading and builds it with `=..`
instead (`dot_term/3`).

**Consequence for surface_dcg:** if the surface keeps `x.field`, the lexer owns
`.` completely and cannot delegate to prolog's reader for anything. Combined
with `|>`, `!rel`, `Entry { .. }` and `match x { .. }`, all of which also fail
to read (checks `bang_negation_is_unlexable`,
`struct_and_match_blocks_do_not_read`), the count of spec constructs that force
a bespoke lexer is five. Prolog-as-the-reader was already dead; the pipe is not
what killed it.

## 2. Desugar and semantics

Input chain (program `pipe_feed`, the ghcacher shape):

```
change_log(Endpoint, Stars, Client) <+
      watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
      fetch(Endpoint, PrevTag, Bucket, Result)
    |> Result = fresh(_Tag, Body), stars_of(Body, Stars)
    |> subscribed_to(Client, Endpoint).
```

Output, verbatim from `-g report`:

```
cut(1, yield,       source_stage(fetch/4))
cut(2, edge_append, head(change_log/3))

pipe_change_log_1(Endpoint, Result) <+
    watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
    fetch(Endpoint, PrevTag, Bucket, Result).
pipe_change_log_2(Endpoint, Stars) <+
    only(pipe_change_log_1(Endpoint, Result)),
    Result = fresh(_Tag, Body), stars_of(Body, Stars).
change_log(Endpoint, Stars, Client) <+
    only(pipe_change_log_2(Endpoint, Stars)), subscribed_to(Client, Endpoint).
```

`only/1` is this lab's spelling of the per-atom trigger marker LANG.md does not
have. It is what makes the R5 claim testable at all.

Graded:

- `chain_desugars_to_three_rules`: variant equality against the term above,
  including variable sharing across the three rules.
- `desugared_trace_equals_hand_written`: program `hand_feed` writes the same
  three rules with human-chosen names (`demand_row`, `folded_row`) and hand
  typed `only/1`. Both traces emit `[[],[],[],[],[+change_log(cli,42,alice)],[]]`
  on `change_log/3`.
- `keyed_head_chain_replaces`: program `pipe_cache` heads a `Key(1)` rel; the
  final write is `-cache(cli,no_tag) / +cache(cli,tag_w1)`.

### 2a. A pipe costs a tick, and that is the point

Check `pipe_stage_costs_one_tick`. The fetch response is injected at tick 3.
`pipe_change_log_1` is written at tick 3, `pipe_change_log_2` at tick 4,
`change_log` at tick 5. Two boundaries, two extra ticks.

This is not sequencing sugar. `|>` is latency, made syntactic. It is also the
reason the boundary law is coherent: if a chain of pure level stages were
allowed, the desugarer would insert intermediate rels that add ticks to a
computation that has no time in it, and the same program written with commas
would be strictly faster and strictly equivalent.

## 3. Binding flow: one rule, inferred

**The rule.** A variable crosses boundary k when it is bound upstream of k
(stages 1..k) AND referenced downstream of k (stages k+1..N, or the head).
Column order is order of first appearance upstream. Nothing is declared.

Graded consequences:

| property | check | receipt |
|---|---|---|
| minimal arity | `carried_columns_are_minimal` | stage 1 binds 4 variables; `pipe_change_log_1` has arity 2. `PrevTag` and `Bucket` are referenced nowhere downstream and are not columns |
| a variable used two stages later still flows | `variable_skipping_a_stage_still_flows` | `Endpoint` is bound in stage 1, absent from stage 2, used in stage 3 and the head. It is a column of `pipe_change_log_2` anyway, and the final row is `change_log(cli, 42, alice)` |
| name reuse across stages | `name_reuse_across_stages_is_a_join` | `Endpoint` reappears in stage 3 as `subscribed_to(Client, Endpoint)`. `bob` subscribes to a different endpoint and does not appear in the output |
| head variable bound nowhere | `head_variable_bound_nowhere_is_rejected` | `unsafe_head_variable(out(_,_))` at desugar time |

**Ruling on shadowing: reuse of a name across stages is a JOIN, and there is no
shadowing.** Justification, in order:

1. It is what the surrounding language already means. Inside a stage, two atoms
   sharing a name join. Making the pipe change that would give one identifier
   two meanings depending on which side of a `|>` its other occurrence is on.
2. Explicit tuple syntax was the alternative (`|> (Endpoint, Result) |>`). It
   was rejected on the skip case: `Endpoint` is used in stages 1 and 3 but not
   2, so an explicit tuple at boundary 2 would have to relist a variable that
   the stage it guards never mentions. Every author would get that wrong once,
   and getting it wrong is a silent join failure rather than an error.
3. Shadowing is not expressible anyway. Under inference there is no way to
   write "a fresh variable that happens to be spelled the same", because the
   chain is one clause and the reader has already unified the two occurrences
   before the desugarer runs. Rejecting it would mean rejecting the join,
   which is the useful case.

Cost of the ruling, stated: the generated rel's arity is invisible in the
source. A reader of the chain cannot see that `pipe_change_log_1` has two
columns without recomputing the variable sets. Ambiguity 8 below.

## 4. The boundary law

Implemented as `cut_evidence/6`, evaluated per boundary, in order:

1. source stage mentions a rel declared `effect(...)` -> `yield`
2. source stage mentions a rel declared `append(...)` -> `edge_append`
3. last boundary and the head's rel is declared `keyed(...)` -> `key_replace`
4. last boundary and the head's rel is declared `append(...)` -> `edge_append`
5. otherwise `throw(no_time_cut(Index, Stage))`

Graded: `cut_kinds_are_yield_then_edge_append` (the legal chain, both cut kinds
named), `chain_without_cut_rejected`, `cut_law_depends_on_declarations`.

### Is the law decidable at desugar time?

**Yes, and only with the rel declarations in scope.** It is not a property of
the chain text. Check `cut_law_depends_on_declarations` proves it with two
programs holding a variant-identical chain:

```
hot(Endpoint) <+ watch(Endpoint), cache_tag(Endpoint, Tag) |> Tag \== no_tag.
```

Under `pipe_no_cut` (no declarations): `no_time_cut(1, (watch(_), cache_tag(_,_)))`.
Under `pipe_declared_cut` (one added fact, `append(cache_tag/2)`):
`cut(1, edge_append, source_stage(cache_tag/2))` and a clean two-rule desugar.

**Exactly what the desugarer must be given**, per rel mentioned in any stage and
per rel headed by any chain:

| information | source in the surface | used for |
|---|---|---|
| is this rel an effect | the signature arrow, `rel fetch(a, b) -> R;` | yield cuts |
| is this rel keyed, and on which columns | `Key(Type)` in the column type position | key-replace cuts, and the interpreter's replace-vs-add |
| is this rel edge-headed (append-only) | the arrow of the rules that head it | edge-append cuts |

The third is the awkward one: it is a property of OTHER rules, not of the rel
declaration. Either the surface gains a storage marker on the rel declaration
(the audit's `Set` vs `Log` typing from finding 5 would supply exactly this and
would close the mixed-head hazard at the same time), or the desugarer runs after
a whole-program pass that classifies every rel by the arrows heading it. The
second option makes desugaring non-local, which is a real cost and a reason to
prefer the first.

Consequence for the tier order: `|>` cannot be a purely syntactic rewrite. It
sits after the declaration table exists, which means after T0 (Key) and T5
(effect signatures), and it needs a rel storage kind that the spec does not
currently have.

## 5. When do commas come back, exactly

Grammar level, no hedging:

```
rule         ::= head ARROW chain
ARROW        ::= '<-' | '<+'
chain        ::= stage ( '|>' stage )*
stage        ::= item ( ',' item )*
item         ::= atom
               | '!' atom                 (negation)
               | comparison               (Stars > 100, Tag != no_tag)
               | binding                  (Result = fresh(Tag, Body))
```

Read off that grammar:

- **Comma is the only within-stage separator. `|>` is the only between-stage
  separator. They never compete**, because their priorities are disjoint:
  1000 (comma) < 1100 (pipe) < 1150 (rule arrows). Every parse in section 1b is
  a consequence of those three numbers.
- **Negation and comparison are ITEMS, so they are comma-joined like any atom,
  and a stage may hold any mix of them.** Graded: `stage_holds_negation_and_comparison`
  runs program `pipe_guard`, whose second stage is
  `Result = fresh(_Tag, Body), stars_of(Body, Stars), Stars > 100, not(muted(Endpoint))`,
  four items across three item kinds. Three scenarios: fires at 420 stars
  unmuted, silent at 420 muted, silent at 42 unmuted. Neither negation nor
  comparison is affected by the pipe in any way; they are inside a stage, which
  is inside a single time cut, which is exactly where they were before.
- **Negation never crosses a pipe.** `!rel` inside stage k is evaluated against
  stage k's time cut. There is no syntax for "absent at the previous cut", and
  the pipe does not create one. That is unchanged from LANG.md, where absence
  needs a reference clock (`ARCH.pl:167`).
- **The head is not a stage.** It is written by the last stage. A chain is not
  `head <- body`; it is `head <- stage_N` with N-1 rules in front of it.

### May a stage contain its own nested pipe?

**Recommend no.** It parses (check `nested_pipe_parses_but_desugar_rejects`
reads `out(X) <+ watch(X) ~> (every_300(B) ~> fetch(X)) ~> true` into a clean
`<+`/2 term), so the rejection is a desugar-time check
(`nested_pipe_in_stage/1`), not a grammar one. Three reasons:

1. **The law loses its meaning.** "A `|>` crosses a time cut" is checkable
   because a boundary has one source stage and one destination stage. A nested
   pipe inside stage k means stage k spans two cuts, so the OUTER boundary's
   cut evidence is being read off a stage that is not at a single instant.
2. **Column inference stops being one rule.** Carried columns at the outer
   boundary would have to be computed against a source stage whose own
   variables are split across sub-cuts, and "bound upstream" no longer has a
   single meaning.
3. **It buys nothing.** `A |> (B |> C) |> D` and `A |> B |> C |> D` produce the
   same rules under any sane flattening. The only thing nesting could express
   is grouping, and there is no operator that grouping would disambiguate,
   because `|>` is the only thing at its priority.

Flattening silently (treating the nesting as a no-op) was the alternative and is
worse: it accepts a program whose author clearly meant something the language
does not have.

## 6. Ramifications of straying from prolog's clause shape

| item | what prolog actually does | verdict |
|---|---|---|
| `\|>` between comma and the rule arrows, 1100 xfy | one `op/3` line, reads correctly, right-associates | **works-as-terms** |
| the glyph `\|>` itself | `\|` is solo; never fuses with `>`; only `'\|>'` quoted reads | **forces-DCG** |
| `!rel` negation | `!` is the cut atom; `!seen(X)` is `syntax_error(operator_expected)` | **forces-DCG** |
| `x.field` dot access | reads as SWI dict `'.'/2`; a space after `.` silently truncates the clause; `./3` throws `type_error(dict, ...)` at runtime | **forces-DCG** |
| `Entry { tag, .. }` struct patterns | no juxtaposition in prolog; `syntax_error(operator_expected)` at the `{` | **forces-DCG** |
| `match x { 200 => a, 304 => b }` | `=>` is SWI's SSU operator at 1200 xfx and cannot sit under a comma inside `{}`; `syntax_error(operator_clash)` | **forces-DCG** |
| a chain is not `head <- body`: one arrow, N writes | the TERM reads fine; nothing in prolog objects | **needs-term_expansion** |
| generated intermediate rel names | `format(atom(...))` plus `=..`; `findall/3` copies terms and severs variable sharing, so carried columns must be recomputed in one pass (`reattach_variables/4`) | **needs-term_expansion** |
| carried-column inference | `term_variables/2` gives first-appearance order for free | **needs-term_expansion** |
| the boundary law | not decidable from the chain term; needs the declaration table | **needs-term_expansion + a declaration pass** |
| nested-pipe rejection | parses cleanly; only a desugar check can refuse it | **needs-term_expansion** |
| edge rules cascading at all | merge_family's tick fires edge rules on outside arrivals only; a chain past two stages never moves without carrying each tick's writes forward | **semantic change to the tick, not a syntax question** |

Reading of the table: the pipe adds exactly ONE thing to the reader's burden
(a glyph that does not lex) and that column already had four entries. Everything
the pipe needs beyond that is `term_expansion` work, which `desugar_machinery`
is already `done` and `rel_island.pl` already proved.

## Ambiguities found (numbered, this lab's space)

1. **`|>` is unlexable in every ISO/SWI prolog.** `|` is a solo character. No
   `op/3` declaration changes that, and the quoted `'|>'` form is not a surface
   anyone would write. Any prototype must use a stand-in glyph.
2. **The pipe's priority chooses which language you get, and the spec does not
   say.** Below comma: a stage is one atom. Between comma and the arrows: a
   stage is a comma body with one head for the chain. Above the arrows: the
   pipe sequences whole rules and each stage carries its own head. All three
   read; only one of them is the proposal.
3. **One arrow token, N writes.** `Head <+ chain` declares the storage kind of
   the final write only. The N-1 intermediate writes have no syntax. This lab
   makes them all `<+`; the spec must state the rule or admit per-boundary
   arrows, which the 1150 clash rules out at this priority.
4. **Whitespace after `.` silently shortens a chain.** `... |> cache(X). tag`
   reads as a legal shorter chain plus a stray clause. No diagnostic exists
   anywhere in the pipeline that would catch it.
5. **Five spec constructs already force a bespoke lexer**, not one. `|>`,
   `!rel`, `x.field`, `Entry { .. }`, `match x { .. }`. `surface_dcg` owes a
   raw-text token (already recorded by the astgrep lab) AND all five of these.
6. **Are rows written by an edge rule arrivals for downstream edge rules?**
   LANG.md does not say. merge_family's interpreter says no, and under that
   reading a three-stage chain never reaches stage 3. This lab carries each
   tick's writes into the next tick's arrival set, which makes `|>` cost
   exactly one tick per boundary. The alternative (cascade to fixpoint within
   the tick) makes `|>` free, which erases the boundary law's justification.
   This must be decided before edge rules ship, with or without the pipe.
7. **The boundary law is a property of the declarations, not of the text.** The
   same chain is legal or illegal depending on one `append(...)` fact. Nothing
   in the spec currently declares a rel's storage kind; it is inferred from the
   arrows of the rules that head it, which makes the check non-local.
8. **Carried arity is invisible in the source.** Inference is the right rule,
   and its price is that a reader cannot see the generated rel's arity without
   recomputing two variable sets. The claimed bonus "mode and cardinality
   thread visibly through stages" is half true: the STAGES are visible, the
   columns crossing between them are not.
9. **R5 survives inside stage 1.** The pipe gives every rule after the first a
   single trigger atom. The first rule of every chain keeps the any-atom
   trigger over its whole body. In the ghcacher chain that body is four atoms,
   including the clock.

## Does it resolve R5?

**Partially, and the measurement is exact.** Check
`trigger_marker_is_what_stops_backlog_replay` runs the same six-tick schedule
against `pipe_feed` (markers generated) and `unmarked_feed` (identical rules,
markers removed), with a late subscriber arriving at tick 6:

| tick | marked | unmarked |
|---|---|---|
| 5 | `+change_log(cli, 42, alice)` | `+change_log(cli, 42, alice)` |
| 6 | (nothing) | `+change_log(cli, 42, carol)` |

Carol connects after the fact. Without a trigger marker her arrival re-fires the
last rule against the standing intermediate set and replays the backlog. That is
R5, reproduced, and the pipe stops it.

What the pipe does NOT stop: check `piped_atom_is_the_only_trigger` deliberately
grades `[_ | Downstream]`, skipping the first rule, because the first rule has
no marker to grade. `pipe_change_log_1` triggers on any of
`watch`, `cache_tag`, `every_300`, `fetch`. A `watch` row arriving at tick 900
re-fires it against every standing fetch response.

So: **`|>` resolves R5 at every boundary and at no stage head.** A ruling on
per-atom trigger marking is still needed, and if that ruling produces a marker
in the surface, the pipe's contribution shrinks to "you do not have to type it
at boundaries".

## The `sugar/2` fact it would register

The pipe grounds into ordinary rules over ordinary terms, plus one thing that is
not a registered primitive:

```prolog
sugar(temporal_pipe, [rule, ground_terms, trigger_marker]).
```

`trigger_marker` has no `kernel/1` or `sugar/2` entry today. `grounds/1` on the
line above fails, which is the correct answer: the registry refuses the feature
until R5 is ruled. Two ways out, both requiring a decision this lab does not get
to make:

- R5 lands a surface trigger marker. It becomes `sugar(trigger_marker, [rule])`
  (an arrival is a delta row joined as an ordinary atom) and the pipe registers
  as `sugar(temporal_pipe, [rule, ground_terms, trigger_marker])`.
- R5 is ruled "first atom is the trigger, positionally". Then the marker is not
  a construct, the pipe generates ordinary rules whose first atom happens to be
  the intermediate, and it registers as
  `sugar(temporal_pipe, [rule, ground_terms])` with no new primitive at all.

The second is cheaper and the pipe makes it more attractive, because a generated
rule's first atom is always the piped-in intermediate by construction, so the
positional rule costs the author nothing at boundaries.

## Tier placement

**T4, after `<+` and `Key(Type)`, and not shippable before T5.**

- The boundary law's `yield` evidence needs effect signatures, which are T5.
- Its `key_replace` evidence needs `Key(Type)` runtime semantics, which are T4.
- Its `edge_append` evidence needs a rel storage kind the spec does not have.
- The whole construct is meaningless in the timeless fragment: T0 through T3
  have no cuts, so every `|>` in them would be rejected by its own law.

Which is the strongest argument for the construct and also for deferring it: a
language where `|>` is legal only across time makes the timeless 90% of the
corpus visibly comma-only, and a reader can tell at a glance whether a rule
touches time. It is worth nothing until the temporal tier exists.

## Deviations from LANG.md

| deviation | why |
|---|---|
| `~>` written for `|>` | `|>` does not lex (ambiguity 1). Priority and associativity are glyph-independent. |
| `only/1` trigger marker | LANG.md has no per-atom trigger. Without one, R5 is not statable and the pipe's main claim is not testable. Hand-written comparison programs write it out by hand so the trace comparison is fair. |
| `not/1` for `!rel` | `!rel` does not lex (check `bang_negation_is_unlexable`). |
| effect demand rows not modeled | The lab injects the fetch response as an arrival at a later tick. The boundary law cares that a cut exists, not how the demand row was addressed; content addressing is the shell_stream and demand_clocking labs' territory. |
| the tick carries edge writes forward | merge_family's tick does not, and under it a three-stage chain stalls at stage 2. Ambiguity 6. |
| `pipe_<head>_<k>` naming | Any generated name works. This one collides with a user rel named `pipe_change_log_1`, which a real implementation must prevent (reserved prefix, or gensym). |
