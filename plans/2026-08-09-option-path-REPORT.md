# option(T) implementation-path scout: receipts and recommendation

Lab: OPTION-LAB.md at repo root (worktree lane, base d88d2ced).
Design settled by plans/2026-08-08-option-type-design.md + rulings.pl
`option_surface` (user 2026-08-09). This report decides WHERE the desugar
runs and proves the winner's first step end to end.

## TOC
- Recommendation
- The three candidate paths
- State before any patch (probe receipts)
- Path B: executed first step, receipts
- Path A: killed, evidence
- Path C: executed cascade probe, killed, evidence
- Fixtures and their rx lowerings
- Blast radius table
- Open risks and unfinished edges
- Commits

## Recommendation

**Path B wins: a 0_option_expand.pl expansion phase at order 5, before enum.**
The first step is landed and green on every gate slice (conformance 334/0,
plunit 496/0, sweep manifest +4 with 3 compiled, TEXT_DOOR 234/234
byte-identical, ARCH green; every leg under 6s). Commit 09a0b5ef is directly
liftable.

## The three candidate paths

| path | mechanism | verdict |
|---|---|---|
| A | parse_dl.pl desugars at parse time | KILLED: text door only |
| B | expansion phase 5 desugars; both doors share it | WINNER, landed |
| C | 0_type_plane.pl carries option(T) as real storage through lower/emit | KILLED: 3 walls deep in one probe, 9-file minimum blast radius, no capability over B |

Both doors run the same expansion fold: compile.pl:170
(`expand_program_with_bindings`) for the text/compile door and
conformance engine.pl:548 (`expand_program`) for the oracle door. Any
desugar placed in 1_expansion.pl is therefore door-symmetric by
construction; failure-modes class 44 (docs/failure-modes.md:1724) is the
receipt for what an untested door split costs.

## State before any patch (probe receipts)

| door | input | result | throw site |
|---|---|---|---|
| term | `column_storage([], option(text), S)` | `unsupported_construct(column_type_unknown(option(text)))` | 0_type_plane.pl:127-128, fall-through default clause |
| text | `email: option(text)` | `dl_parse_error(statement, position(1,45))` | parse_dl.pl typed_column_type bare-ident fallback ate `option`, left `(text)` |
| text | `note: text?` | `dl_parse_error(statement, position(1,43))` | no rule consumed `?` |

All three are unfinished work, not impossibilities: a missing clause each,
no structural refusal anywhere.

## Path B: executed first step, receipts

### Files touched (commit 09a0b5ef, +63/-12 across 6 files + outputs)

| file | change |
|---|---|
| v6/prolog/0_option_expand.pl | NEW, 118 lines: the desugar |
| v6/prolog/1_expansion.pl | phase `5-option` registered; enum context computed from option-expanded decls (idempotent pre-pass) |
| v6/prolog/compile/parse_dl.pl | typed_column_type split into wrapper + `typed_column_type_base`; `option(T)` clause; `T?` suffix; retained term is `option(T)` in both spellings |
| v6/prolog/compile.pl | reserved-namespace check allows body READS of `__opt_*` (declaring or head-writing one stays refused) |
| v6/prolog/compile/test/plunit_tests.pl | two pinned expectations regenerated with intent: phase list gains 5-option; corpus plane counts refcount/refcount_staging 281 -> 286 |
| v6/prolog/conformance/fixtures/0_option_type.pl | NEW: 4 fixtures (below) |

### Desugar shape (matches the design doc exactly)

- scalar element: mint `enum_decl('__opt_<t>', (none ; some(value:<t>)))`
  once per element type (ruling 3), retype the column to the enum name;
  enum phase 10 then owns variant rels, tag rel, tag rules, and the int
  retarget. Zero new IR past phase 10.
- rel-ref element: drop the column, shrink the parent ref one arity,
  renumber key positions past the dropped column, append
  `<parent>__<column>(<parent>_id: int, <element>_id: int) key(1)`.
- named refusals minted: `option_in_key_column/2`,
  `option_column_untyped_siblings/1`, `option_of_enum_unsupported/1`,
  `option_element_type_unknown/1`.

### Gate results (all from this worktree, wall times per run)

| gate | command | result | wall |
|---|---|---|---|
| conformance | `swipl -q -l conformance/go.pl -g go -g halt` | 334 PASS / 0 fail (330 baseline + 4 new) | 0.30s |
| plunit | `swipl -q -l compile/test/plunit_tests.pl -g run_tests -g halt` | 496 tests, 0 failed | 5.9s |
| sweep | `bash scripts/sweep.sh` (v6/tsv2) | manifest 334 entries; ADDED 3 compiled + 1 unsupported (the key-ban fixture, correctly); replay RUN total=234 identical=233 wrong=0 (the 1 rejection is the pre-existing log_retraction_rejected); restated=0 | 5.0s |
| TEXT_DOOR | `bash compile/scripts/text_door_receipt.sh` | compiled=234 byte_identical=234 failures=0 | 5.8s |
| ARCH | `swipl -g go -t halt ARCH.pl` | green | 0.03s |

10-second law: every leg under 6s.

### Sabotage receipt (the oracle really grades these)

Flipping fixture 1's expected final from `email_state(1, some)` to
`email_state(1, none)` turns conformance red:

```
MISMATCH final email_state/2
  got [email_state(1,some)]
  want [email_state(1,none)]
fail  option_text_column_reads_through_tag_join
FAILURES  1
```

Restored, back to 0 failures. ticklog.pl grades the fixtures with zero
special cases: by grading time the program is ordinary rels.

### Catalog rows (__rel plane)

The emitted module's `rel_catalog` const carries the minted enum instance
as ordinary rows: `__opt_text_none` (rel, arity 1, id column int),
`__opt_text_some` (rel, arity 2, value column type_id 1 = text),
`__opt_text_tag` (rel with h_rule set, the tag rule). Receipt:
out/option_text_column_reads_through_tag_join.ts:217-230.

### Emitted DDL (surrogate-keys law compliance)

```
CREATE TABLE "__opt_text_some" ("id" INTEGER NOT NULL, "value" INTEGER NOT NULL, PRIMARY KEY ("value")) WITHOUT ROWID
CREATE TABLE "user_profile" ("user_id" INTEGER NOT NULL, "email" INTEGER NOT NULL, PRIMARY KEY ("user_id")) WITHOUT ROWID
CREATE TABLE "commit__reviewed_by" ("commit_id" INTEGER NOT NULL, "person_id" INTEGER NOT NULL, PRIMARY KEY ("commit_id")) WITHOUT ROWID
```

Every key INTEGER; text lives once in the `__str` dictionary. No composite
TEXT PK anywhere (checked against .claude/skills/sql-relational-design).

## Path A: killed, evidence

A parse-time desugar lives only behind parse_dl.pl. The oracle door never
parses text: engine.pl:548 expands the fixture TERM directly, so under
Path A no conformance fixture could spell option(T) at all and the graded
record could never cover it. The text-door receipt also prints term
fixtures back to .dl6 (print_dl.pl), so the sugar must survive as a
retained term anyway for the round trip. The parse clause Path A needs is
the same one Path B needs; everything else A adds is a second copy of the
desugar in a door-local place. A is strictly B minus the term door.

## Path C: executed cascade probe, killed, evidence

Transient patch sequence (not committed; run with phase 5 unwired so
option(text) reached the type plane raw):

| step | patch | next wall |
|---|---|---|
| 1 | none | `column_type_unknown(option(text))` at 0_type_plane.pl:127 |
| 2 | `column_storage(_, option(Element), option(Element))` clause | `column_type_unknown(option(text))` again, from the SECOND registry: 0_program_check.pl:342-346 (`program_violation(column_type_unknown, ...)` keeps its own legal-type list) |
| 3 | `\+ Name = option(_)` added there | `decl_type_conflicts_witness(user_profile/2, 2, option(text), int)` from the analyze.pl typing fixpoint: the wall is now SEMANTIC (what is an option value at the storage plane), which is exactly the question the enum desugar answers for free |

Beyond wall 3 by the list(T) precedent (the one existing parametric type):
9 non-test files pattern-match a parametric column type
(0_program_check.pl, 0_type_plane.pl, analyze.pl, parse_dl.pl,
0_json_arrival.pl, emit_ts.pl, lower.pl, print_dl.pl, sweep.pl; 34 direct
`list(Element)`/`list(_)` sites), plus the oracle's canonicalization and
the ts boundary decode. Path C buys none of this back as capability: the
design already fixes storage as an int instance id, which is what B emits
today. C stays the right shape only for a future where option needs its
own printer mappings per target (ts `T | undefined`, jsonschema
absent-from-required); that is a boundary-decode arc on TOP of B's
storage, not a competing entry point.

## Fixtures and their rx lowerings

### 1. option_text_column_reads_through_tag_join (scalar, arrivals + retraction)

Surface (.dl6 spelling; both ruled spellings parse):

```
rel user_profile(user_id: int, email: option(text)) key(1).
rel email_state(user_id: int, state: text).
email_state(UserId, State) <-
    user_profile(UserId, EmailOption), __opt_text_tag(EmailOption, State).
```

Intended pure-rxjs lowering:

```js
const emailState$ = combineLatest([userProfileRows$, optTextTagRows$]).pipe(
  map(([profileRows, tagRows]) =>
    profileRows.flatMap((profileRow) =>
      tagRows
        .filter((tagRow) => tagRow.id === profileRow.email)
        .map((tagRow) => ({ user_id: profileRow.user_id, state: tagRow.tag })))),
  distinctUntilChanged(sameRowSet)
);
```

Graded: some arrives (tick 2 delta `+email_state(1, some)`), none arrives
(tick 4 `+email_state(2, none)`), retraction (tick 5 `-email_state(2, none)`).

### 2. option_scalar_enums_mint_per_element_type

option(text) and option(int) in one rel land on two distinct instances
(`__opt_text_tag` vs `__opt_int_tag`), receipt for ruling 3 (one enum per
element type) and ruling 2 (ids never compare across types: separate
tables). Rx lowering: two independent tag streams, no shared subject.

### 3. option_rel_ref_desugars_to_companion_split_rel

```
rel person(id: int, name: text) key(1).
rel commit(id: int, reviewed_by: option(person)) key(1).
rel reviewed(commit_id: int, reviewer_name: text).
reviewed(CommitId, ReviewerName) <-
    commit__reviewed_by(CommitId, PersonId), person(PersonId, ReviewerName).
```

Intended pure-rxjs lowering:

```js
const reviewed$ = combineLatest([commitReviewedByRows$, personRows$]).pipe(
  map(([reviewLinkRows, personRows]) =>
    reviewLinkRows.flatMap((reviewLink) =>
      personRows
        .filter((personRow) => personRow.id === reviewLink.person_id)
        .map((personRow) => ({
          commit_id: reviewLink.commit_id,
          reviewer_name: personRow.name })))),
  distinctUntilChanged(sameRowSet)
);
```

Absence is the empty inner match, never a null. Graded: presence arrives
(tick 3 `+reviewed(101, "ada")`), retraction (tick 4). `commit/1` (the
shrunk parent) finals confirm the arity rewrite.

### 4. option_in_key_column_is_refused

`throws(unsupported_construct(option_in_key_column(session/2, token)))`,
the design doc's named ban. Lands in the sweep manifest as `unsupported`
with that reason (stage 4 ADDED line is the receipt).

## Blast radius table

| path | files touched for the first step | files remaining for full surface |
|---|---|---|
| A | parse_dl.pl (+ its own desugar copy) | term door NEVER covered |
| B | 5 source files + 1 fixture file | boundary printer mappings (ts/jsonschema) when that arc opens; sh_decl/bind option columns |
| C | 3 files patched before the semantic wall | >= 9 files + oracle + ts runtime |

## Open risks and unfinished edges

- `T?` prints back canonically as `option(T)` (print_dl default clause);
  round-trip identity holds because the receipt compares emitted TS bytes,
  never surface text bytes.
- option in sh_decl host columns and bind columns parses and then falls to
  the existing `column_type_unknown` refusal; unfinished work, named, cited.
- Nested constructors (`option(option(T))`, `option(json_list(T))`,
  `json_list(option(T))`) parse and refuse downstream with named reasons;
  no fixture pins the exact reason text yet.
- The `__opt_*` read allowance in compile.pl is prefix-based; the oracle
  door has no reserved-namespace check at all (pre-existing asymmetry, now
  load shared by option reads; a term fixture writing `__anything` still
  diverges between doors, unchanged from before this lab).
- match exhaustiveness over a minted option enum flows through the
  enum-context pre-pass in 1_expansion.pl; no fixture exercises match on
  `__opt_text_tag` yet.
- `option_column_untyped_siblings`: the desugar requires every sibling
  column typed (position arithmetic needs it); cheap to lift later.
- plunit's two pinned counts were regenerated with intent (phase list,
  corpus plane counts 281 -> 286); recorded here as the regen receipt.

## Commits

| commit | content |
|---|---|
| 34d10405 | OPTION-LAB.md contract |
| 09a0b5ef | Path B first step, all files + regenerated out/ |
| (this doc's commit) | reports, lab results |

Path C probe patches were transient and reverted in-tree; their full
receipt is the cascade table above.

## Lab cull
OPTION-LAB.md (contract + result log) last copy: commit 386d53d6, merged in dafaaf46 (PR #69).
