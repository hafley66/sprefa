# Duel: dl6 structural-query lowering target

Base 92756b54. Ground truth read: `1_host_expand.pl:151-165, 404-489`,
`README.md:69, 312-313, 505, 557`, `v6/prolog/compile/parse_dl.pl` (1655 lines),
`v6/prolog/LANG.md`, `v6/README.md:1-27`.

## 1. Ruling

**STRING, with the boundary drawn at the declared-host contract.** The S-expr
sugar is parsed by a DCG (precedent: `parse_dl.pl`, the same DCG-based term
surface), normalized into the exact term vocabulary that `ts_pattern_text/2`
already lowers (`1_host_expand.pl:424-474`), emitted to query text by
`compile_ts_query/2` (`414-422`), and served through the existing declared-host
path (`compile_host_decl/2`, `181-192`; `compile_query/2`, `404-412`) as a
visible op rel whose captures are columns. The compiler sees the op's schema
(visible contract); it stays blind to match internals, which is precisely how
every other host executor is already treated (`validate_host_executor/3`,
`194-199`, inspects the executor's column contract, not its semantics).

Why STRING, not JOINS:

1. **Buy, don't build** (`v6/README.md:26`, restated as "buy-dont-build" at
   `:9`). The tree-sitter matcher is a bought library. JOINS re-derives ordered
   structural matching semantics inside datalog — hand-rolling a library that
   exists, the exact thing rule 1 forbids.
2. **The emitter already exists.** `ts_pattern_text/2` (`1_host_expand.pl:424-
   474`) already covers node kinds, named children, fields, captures, capture
   refs, `#eq?`/`#match?` predicates, `?`/`*`/`+` quantifiers, alternations,
   anonymous/string literals, and both wildcards. STRING adds no matching
   machinery.
3. **JOINS would need semantic surface that does not exist.** `child/2` is
   2-col (`README.md:505`) with no field name; `node/7` carries `lo,hi,parent`
   spans (`README.md:557`). tree-sitter fields (`body:`, `value:`) are named
   child edges — not recoverable, requiring a new `(parent, field, child)`
   rel. Sibling ordering and anchor `.` are not indexed joins; interleaved
   quantifiers become correlated streak predicates, not joins.
4. **Visible lowering is satisfied at the op level.** LANG.md's sugar-with-
   visible-lowering doctrine (`LANG.md:22` "effects = one rel") is met: the
   sugar lowers to a declared host rel with a fixed column schema and capture
   bindings, as visible as any other source op. STRING is not an opaque blob;
   it is a declared rel whose columns are the pattern's captures.

The boundary is exact: everything up to and including the emitted query text is
compiler-visible and refused at compile time when not in the emittable
vocabulary (the `unmapped_feature/2` throws at `420, 422, 474`). Past the text
atom, matching is the host's, column-style like every other executor.

## 2. Steelman against the ruling

The strongest concrete failure for STRING is the **field-and-anchor query that
JOINS alone can push deterministically**:

```
(function_declaration
  name: (identifier) @name
  .
  body: (block (return_statement
    (call_expression function: (identifier) @callee))))
```

Requirement: this exact match (name immediately followed by body with no
intervening sibling, a specific named parameter, the callee captured). STRING
hands the text to the host and gets back `(file, span, callee)` rows. The
compiler cannot tell the author, before running, that the trailing clause is
redundant given the anchor, or that `body:` is absent from the grammar, or
fold the pattern against a known `kind` universe. Want it scoped to "only
functions that are also type entities"? STRING forces a capture then a
downstream `type_entity` join; JOINS would have bound the node id into the
type plane directly. And per-refresh, STRING reruns the host matcher on every
tick, where a JOIN over a materialized CST would delta-increment in the
cascade. The severity is real: STRING forfeits compile-time shape reasoning and
incremental recompute, and the `error`/ordering corners make it hard to predict
cardinality.

The counter that holds: incremental recompute and kind folding are bought back
only by materializing the full CST (node + child + a new field edge rel) into
the store and re-implementing sequence-anchored matching, which is the
point-of-failure (Section 4) and directly violates rule 1. The `type_entity`
scoping is a one-line downstream join on the `@name` capture; it is not lost,
just deferred one hop.

## 3. Falsifying experiment

Smallest lab that flips the ruling to JOINS.

- **Method.** Single Prolog lab (LANG.md lab style: `swipl -q -l <lab>.pl -g
  go -g halt`). Hand the field-and-anchor query above to both lowerings: (a)
  STRING via `ts_pattern_text/2` -> the existing host; (b) JOINS as a DL rule
  set over `node/7` + `child/2` + a candidate `field/3` rel. Run both against
  a fixed corpus of ~200 source files. Compare exact match sets: the sorted
  `(file, node_id, name_capture, callee_capture)` tuples.
- **Pass criterion (flips to JOINS).** JOINS reproduces the STRING-host match
  set *exactly* (byte-identical captures) AND it does so adding no more than
  two new rels (a `field/3` edge rel and an ordered-sibling order rel, each
  derivable) AND the sugar-to-JOINS expansion completes in bounded compile
  time (no unbounded fixpoint beyond `closure(child)`).
- **Fail criterion (confirms STRING).** Any one of: JOINS diverges on the
  anchor case (non-adjacent siblings matched), needs a `field/3` rel that
  itself requires a new extractor, expands `*`/`+` over siblings into a
  count-unbounded streak predicate, or cannot reproduce `ERROR`-node matching.
  Divergence on the ordering/field case alone is sufficient to hold STRING.

Because `child/2` has no field column and no sibling-order column today, the
experiment is designed so the pass case requires two new schema surfaces plus a
sequence-aware matcher — the expensive combination.

## 4. Cost (STRING ruling)

- **Files touched:**
  - `v6/prolog/compile/parse_dl.pl` — add a DCG for the S-expr sugar into the
    existing term surface (precedent exists, the file is already a DCG parser).
  - `v6/prolog/1_host_expand.pl` — extend `ts_pattern_text/2` (`424-474`) with
    up to two missing emitters (anchor `.`, and sg-metavariable at `420` if the
    sugar wants `match_ast` parity). Everything else is already lowered.
  - No new `sprefa-store` code. The host matcher and the declared-host contract
    (`compile_host_decl/2`, `181-192`; `host_relation_refs/3`) stay as-is.
- **New machinery:** one DCG block; optional anchor/metavar emitter branches.
  Zero matcher logic, zero new store rels, zero new extractors.
- **Contrast (if JOINS):** a `field/3` extractor + rel, a sibling-order rel +
  its population from `node/7` spans, a sequence-and-quantifier matcher as
  DL rules, plus materializing full CSTs in the store — `README.md:505,557`
  are the seed, every kind/field/order surface beyond that is new cost.
- No behavioral call-site migration: the sugar is additive to the term parser.

## 5. Semantics inventory

Column "STRING" = supported-how / deferred / dropped under the STRING ruling.
Column "JOINS" = what the datalog reimplementation would require (the avoided
cost).

| tree-sitter feature | STRING (ruling) | JOINS (avoided reimplementation) |
|---|---|---|
| node kind `(fn_decl)` | emit via `node/2`, `430-437` | join on `node.kind`; needs kind-precise CST |
| named children (nesting) | `node/2` children, `430-437` | recursive joins over `child/2`; needs materialized CST |
| fields `body:` | `field/2`, `438-441` | **new `field/3` rel**; not in `child/2` col set (`README.md:505`) |
| captures `@x` | `capture/2`+`capture_ref/2`, `442-448`; become columns | capture = a node column; needs per-capture slot plan |
| anchors `.` | deferred: not in `ts_pattern_text` vocab; add branch | sibling-order rel + strict-adjacency join |
| quantifiers `? * +` | `quant/3`, `461-466` | `*`/`+` over siblings = streak/closure, not join (`README.md:557` containment is ancestry only) |
| alternations `[...]` | `alternative/1`, `467-470` | union of branches; ordering combined = blowup + dedup |
| wildcard `_` / `(_)` | `wildcard/1`, `named_wildcard/1`, `471-472` | any-child / any-named-child existence join |
| anonymous/string literal | `anonymous/1`, `string/1`, `449-452` (`ts_quoted`, `480-489`) | text equality against node slice; needs content access |
| `ERROR` / error nodes | node(Type=error); supported only if host CST keeps error nodes | needs error nodes materialized + recovery ordering |
| predicates `#eq?` `#match?` | `predicate/3`, `453-460`; `#match?` = regex on capture | join filter + regex fn; `#match?` needs a regex op |
| sg metavariables `$X` | deferred: today throws `unmapped_feature(slot_sg_metavariable_semantics, ...)`, `420`; add emitter for `match_ast` parity | metavar = unbound var live in the type plane; the one thing JOINS does natively |
| negation `!` | dropped: not in vocabulary, `473-474` throws `slot_ts_pattern_form` | NOT-EXISTS/antijoin (feasible, extra rels) |
| unmapped forms | compile-time throw, `420,422,474` (refusal rather than silent miscompile) | compiler sees everything, can refuse any form before lowering |

Net: every tree-sitter feature except negation, anchors, and sg-metavariables
is already emitted by the existing code. The three missing branches are small
emitter additions (or a deliberate defer). None of them justify rebuilding the
matcher in datalog.
