# TYPES-AS-RELS DESIGN HEADER (planner contract, user go 2026-07-28 PM)

User directive, verbatim intent: "a rational type system that makes rel and
types/enums all almost look the same (i dont care if modeling a type graph
requires N type tables flatly with N edge tables (who is pointing to who),
make safe assumptions of non domination (identity not tied to parent) ...
or do, how would we even indicate that (this is cascade delete logic which
should likely be embedded into the code)."

This un-banks the nested/reference storage header banked by the
typed-columns ruling (rulings.pl 2026-07-28: struct-as-rel + surrogate id,
the intern-dictionary pattern). Deliverable is a DESIGN DOC with priced
options, no implementation.

## Fixed constraints (ruled, do not relitigate)

- Typed columns: int/text INTEGER/TEXT landed; compound terms currently
  inline-flat json1 (`{"fn":..,"args":[..]}`) with canonical-text CASE at
  read. This header designs what that punt becomes; migration path is a
  question, not a given.
- Vocabulary law: rx/prolog/sql words only. SQL already owns most of this
  domain: FOREIGN KEY, ON DELETE CASCADE, PRIMARY KEY, recursive CTE.
  Prolog owns functor/arity/args. rx owns lifetime/teardown framing. No @
  symbols (user 2026-07-28).
- ARCH.pl symmetric struct/tuple discipline: "a rel is a set of structs, a
  struct is a row, terms nest in columns -- one value world, so matching
  gives branching." The design must keep that symmetry or name the break.
- LIFETIME IS DOMINATED callout + plans/2026-07-27-mode-dominance.md:
  domination machinery exists on the subscription axis; question 3 asks
  whether ownership is the same machinery.
- Content-addressed identity is the ruled default for effects
  (salt_minting); the intern dictionary (v5 storage-diet A=1a dense ids)
  is the precedent for value identity.
- Self-host bar (user): "if they got an optional higher type system that
  is also a win" -- the type layer is optional, programs without decls
  keep working.
- RULED mid-header (user 2026-07-28 PM): "a type ref is just always
  surrogates, not even a question." Every nested type position lowers to a
  surrogate id column, never inline. Manual relational references, if ever
  needed, are the exception spelling to price, not the default. The
  center-of-gravity question of this whole header is: what does nesting
  lower to in rels (Q5 is primary, Q2 shrinks to minting policy only).

## Questions (each = priced options + recommendation, exhaustive tables
## where the option space is enumerable)

Q1 DECL UNIFICATION. One spelling family for rel/struct/enum. What
actually differs between `rel route(id, body)` and a struct decl of the
same shape -- storage, identity, subscribability, or nothing? Enum = sum
type: price at least (a) one table + tag column, (b) N variant tables +
shared id space, (c) variant tables + a tag edge table. Include how the
existing envelope enums (FetchResult) land in each.

Q2 MINTING POLICY ONLY (surrogates themselves are RULED, see constraints).
Content-addressed intern (same value = same id, shared rows, the stated
non-domination default) vs opaque minted ids. Where each breaks: mutation
of a shared node, retraction counting (two parents, one child), the Key/Q8
identity-columns thread. Interaction with IVM support counting.

Q3 DOMINATION SPELLING (the cascade-delete question). How to say "child
rows die with the parent row." Price: (a) SQL-native FOREIGN KEY ... ON
DELETE CASCADE emitted in DDL, semantics in the database; (b) domination
as IVM support: the parent edge declared as the child's ONLY support, so
existing retraction machinery cascades with zero new code -- explore
whether cascade delete DISSOLVES into support counting the way Ta may
dissolve into pending rels; (c) explicit edge-table column marking. For
each: what the generated code contains, what the tick log shows when a
parent dies, and the rx lowering (per the snippet law). Surface spelling
options must be no-@ and inside the vocabulary law.

Q4 EDGE TABLES AS RELS. The who-points-to-who tables: are they ordinary
rels -- queryable by dl rules, visible to measures/LSP, subscribe-able?
If yes, the type graph is program data and the everytool bet applies; name
any place that self-reference bites (bootstrapping, migration ticks).

Q5 NESTED MATCH LOWERING. `route.field.sub` destructuring across the
flattened graph: the SQL (joins over edge tables) and rx lowerings at 1,
2, 3 levels deep; where json1 inline stays as a fast path or cache; the
point where join depth costs more than inline duplication (cite the
storage-diet receipts if used).

Q6 MIGRATION AND COEXISTENCE. Per-rel opt-in? Can inline-flat json1 and
reference storage coexist in one program; what the compiler needs to know
per column; what the oracle/tick-log grade looks like during migration.

Q7 RECURSIVE TYPES. Trees/graphs as self-referential edge tables;
querying = recursive CTE (the engine's own fixpoint machinery); where the
not_stratified guard interacts; termination story.

Q8 TYPE-CHECK RESIDENCY. With types as rows, is the checker itself dl
rules over the type tables (self-hosting win, ties to books/v6/algos/
unify_hm.pl) or compiler-side prolog only? Price both.

## Named slots for user rulings

SLOT-DECL-SPELLING (the unified decl surface), SLOT-OWNERSHIP-MARK (the
domination spelling), SLOT-ENUM-SHAPE (sum-type table layout),
SLOT-INTERN-SCOPE (intern per rel, per type, or global),
SLOT-JSON1-FATE (fast path, cache, or removed).

## RESHAPED TO A LAB (user, same afternoon): "this needs a lab to explore
## what combo is the most compact and makes sense, we want beautiful/
## harmonious unity here bc js and json have it, so type could literally be
## a shorthand? enum is a shorthand?"

THE UNIFICATION HYPOTHESIS (primary thing the lab grades): struct/enum/
type are SHORTHANDS over rel. A struct decl = a rel decl with the policy
bundle pre-pinned: identity = content-addressed surrogate, mutation =
never (new value = new id), lifetime = refcount via IVM support. An enum
= N variant rels sharing one id space. Nesting is NEVER physical: a
nested position is a ref column; the tree exists only in the printer
(json view) and the matcher (dot-path = join path). If the hypothesis
holds, the decl surface needs ONE construct plus policy words; if it
cracks, the lab names the row/scenario where a struct cannot be a rel.

Lab checks (self-loading .pl, PASS lines only, lab protocol as in the
match_frontier lab; files in v6/prolog/labs/types_as_rels/, die on
landing):
- JSON ROUND-TRIP: a model json value (nested objects, arrays, enum-
  tagged variants) lowers to term-form tables (type tables + ref columns)
  and prints back BYTE-IDENTICAL. Include a shared-substructure value
  (same subtree referenced twice) proving intern sharing, and the
  round-trip of it (tree view duplicates, graph stores once).
- POLICY-BUNDLE DERIVATION: express one struct table purely as existing
  rel machinery (interned set rel + support-refcount lifetime) in the
  model; grade that its behavior under insert/share/release matches the
  hypothesis table (no new construct needed = PASS).
- DOMINATION SCENARIO: parent dies; (a) shared child (refcount > 0)
  survives, (b) solely-owned child cascades via support-zero. Tick logs
  hand-computed and graded. If cascade needs anything beyond support
  counting, that is a finding, not an assumption.
- MATCH-PATH LOWERING: dot-path patterns at depth 1/2/3 emit join SQL in
  the model; each with its rx lowering string (snippet law); depth-cost
  table.
- COMPACTNESS PRICING: at least 3 decl-surface spellings for the
  shorthand family (js/json-flavored braces, prolog-flavored, sql-
  flavored), each shown on the SAME worked example (route tree with enum
  body), scored on: chars, number of distinct constructs, distance from
  json, distance from current rel decls. No @ symbols. The lab prices,
  the user picks.

## Deliverable

v6/prolog/labs/types_as_rels/*.pl (self-loading, PASS-only) +
plans/2026-07-28-types-as-rels-verdict.md: verdict line first (hypothesis
holds / cracks where), the Q1-Q8 priced tables, the five check results,
slots filled-or-open, and the worked example in all three candidate
spellings with (a) decl surface, (b) generated DDL incl. edge tables,
(c) two rules matching into it with SQL + rx lowerings, (d) tick logs for
the domination scenario pair. No implementation outside the lab dir, no
fixture edits, no engine claims without file:line.

## ROUND 2 CONTRACT (user, 2026-07-28 evening; runner = codex gpt-5.6-sol)

User words, condensed: fixpoint on finding and reaffirming the lab
results; flush out more pros/cons on the ambiguities and fights,
specifically ENTITY vs VALUE OBJECT (ids-as-unique vs content hash) --
the user is NOT SOLD on struct = content-hash-every-column as the
default; content hashes may carry a dense surrogate mate (fine); and the
deliverable must include the ITERATION JOURNAL: how the agent validated
idea N for reason M against ideas N-1..., how it reached the suggested
arrangement, WHY things disagree wherever they disagree, and how both
sides of a dichotomy can coexist WITHOUT implicit defaulting -- e.g.
decomposed into DDL words vs body-language spellings.

Round-2 requirements on top of the round-1 contract:
1. RECOVER round 1: `git checkout b58d1ece -- v6/prolog/labs/types_as_rels/`
   into the worktree; re-run; 36 PASS is the entry bar.
2. FIXPOINT PROTOCOL: rounds of (a) actively try to break every prior
   conclusion with new scenarios, (b) encode new findings as checks,
   (c) stop only when a full round adds zero new findings. Journal each
   round.
3. ENTITY vs VALUE, both first-class, NO implicit default: value object
   = content-addressed, immutable, refcount GC (round 1's plane); entity
   = extrinsic unique id, mutable row history, explicit lifetime
   (keyed/retention machinery), and note entities un-crack round 1's
   cycle finding. For each policy: mutation, sharing, GC/deletion,
   cycles, keyed interplay, tick-log shape. Then price the coexistence
   decompositions: (a) DDL decl word per type, (b) body/use-site
   spelling, (c) hybrid; each with the worked example re-shown.
4. SURROGATE MATE: semantic identity = content hash, storage key = dense
   int via the intern dictionary; validate this reconciles the round-1
   dense-ints-vs-content-ids ruling collision; tick log prints VALUES.
5. Re-affirm or amend every round-1 conclusion under BOTH policies
   (cons lists, four-bit bundle, domination-by-support completeness --
   state where support GC is complete only on the value plane and what
   the entity plane pays instead).

Extra deliverable: plans/2026-07-28-types-as-rels-iteration-journal.md
(the numbered idea/compose/conflict/check log). Verdict file gains a
ROUND 2 section; same PASS-only lab discipline; labs die on landing.
