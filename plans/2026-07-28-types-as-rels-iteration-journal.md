# TYPES-AS-RELS ROUND 2 ITERATION JOURNAL

Contract: `plans/2026-07-28-types-as-rels-header.md`, ROUND 2 CONTRACT.

Lab entry:
`v6/prolog/labs/types_as_rels/types_as_rels.pl`.

## Execution receipt

| round | attack target | new findings | PASS count after encoding |
|---|---|---:|---:|
| entry | recovered round-1 lab at `b58d1ece` | 0 | 36 |
| 1 | apply every value-plane conclusion to mutable extrinsic identity | 10 | 46 |
| 2 | split semantic identity from dense storage identity, then place the policy choice three ways | 10 | 56 |
| 3 | compose value and entity refs, lists, enums, matching, and lifetime | 9 | 65 |
| 3 close | encode the round ledger and stopping condition | 1 check, no additional finding | 66 |
| 4 | repeat the full conclusion matrix under both policies | 0 | 66 |

Round 4 is the stopping round. It actively replayed identity, mutation,
sharing, lifetime, cycles, list shape, enum shape, nested refs, DDL, tick
output, match paths, migration, recursion, checking, and merge. Every scenario
mapped to an already encoded result, so it added zero findings.

## Numbered idea log

### Idea 1: preserve the recovered value plane as one explicit policy

Reason: the 36 PASS entry lab already establishes byte-identical json
round-trip, content sharing, support collection on a DAG, match joins,
three declaration spellings, and the four-bit merge result. Round 2 needs a
second policy without weakening those receipts.

Composes with: round-1 content identity, immutable values, support counting,
cons lists, variant rels, and value-rendered tick output.

Conflicts with: the round-1 wording that described this bundle as the struct
shorthand without requiring a policy choice.

Check: all original 36 checks remain in the entry file and pass before and
after every round.

Result in the final arrangement: the recovered plane is named `value`.
Nothing selects it implicitly.

### Idea 2: model entity as an independent four-part policy bundle

Reason: continuity through change requires an identity outside the current
field values. A content hash changes when any identity field changes.

Composes with: plain `rel`, keyed current-state replacement, stamped history,
and boundary diff.

Conflicts with: same content means same row, immutable update-by-remint, and
support-zero as the only lifetime event.

Checks: `both_policy_bundles_have_four_bits`,
`entity_equal_content_mints_distinct_ids`,
`entity_update_preserves_id`.

Result: entity is extrinsic id, mutable current row, immutable history,
explicit retire, keyed merge.

### Idea 3: require the choice at a declaration or construction boundary

Reason: choosing by shape would silently classify two identical field lists
differently based on compiler inference or ordering. The contract prohibits
that default.

Composes with: both policy bundles and the three coexistence decompositions.

Conflicts with: round-1 optional `value` sugar and any rule that treats an
unmarked compound declaration as value.

Check: `both_policies_require_explicit_decl`.

Result: `decl(route)` does not resolve. `decl(route, value)` and
`decl(route, entity)` do.

### Idea 4: make entity mutation a current-row delta plus a history append

Reason: a stable entity id by itself loses prior state unless update history
is stored separately. Keeping only history makes current reads pay a max tick
query.

Composes with: keyed merge and the round-1 result that monotone keyed state
still emits a retracting boundary.

Conflicts with: value immutability and set merge.

Checks: `entity_update_appends_history`,
`entity_update_boundary_retracts_and_adds`,
`entity_retire_tick_prints_current_row`.

Result: current storage uses one keyed row per entity. History uses
`version(Tick, Type, Id, Args)`. An update emits `-old, +new` for the same id.
Retirement emits the current row and retains history.

### Idea 5: replay the cycle crack with extrinsic ids

Reason: round 1 proved that a value parent hash depends on child hashes, so a
content-addressed graph is a DAG. Extrinsic ids exist before all refs are
known, permitting a later update to close a cycle.

Composes with: entity update and explicit lifetime.

Conflicts with: the unqualified round-1 line that cycles crack the unification
hypothesis.

Checks: `entity_cycle_can_be_constructed`,
`entity_cycle_partial_retire_is_refused`,
`entity_cycle_explicit_set_retires`.

Result: cycles still crack the value policy. They do not crack `rel` as the
common construct because the entity policy represents them.

### Idea 6: make entity retirement an explicit set operation

Reason: refusing retirement whenever a row has an inbound ref makes a cycle
undeletable. Ignoring inbound refs creates dangling refs. A retirement set can
close over a cycle and reject refs entering the set from outside.

Composes with: extrinsic identity and explicit entity lifetime.

Conflicts with: support-zero collection and single-row delete as a complete
entity lifetime operation.

Checks: `entity_cycle_partial_retire_is_refused`,
`entity_cycle_explicit_set_retires`,
`entity_lifetime_does_not_follow_refcount`.

Result: value collection remains automatic and set-at-a-time. Entity
retirement is an explicit atomic set checked for outside referrers. Retention
rules may decide when that operation is requested.

### Idea 7: keep the four-bit bundle and scope each bit by policy

Reason: round 1 found identity, mutation, lifetime, and merge. Entity changes
the selected value for every bit, without adding a fifth semantic bit.

Composes with: round-1 lattice checks.

Conflicts with: treating dictionary visibility as an additional policy bit.

Checks: `both_policy_bundles_have_four_bits`,
`kind_words_are_joins`,
`keyed_state_rises_but_boundary_retracts`.

Result:

| policy | identity | mutation | lifetime | merge |
|---|---|---|---|---|
| value | content hash | immutable | support zero | set |
| entity | extrinsic id | current plus history | explicit retire | keyed |

Dictionary writes stay below the logical boundary. Logical tick output prints
values, so dictionary visibility is a lowering rule rather than a fifth bit.

### Idea 8: give each value hash a dense integer mate

Reason: the round-1 rulings wanted content-derived identity and integer
storage keys. A single column cannot satisfy both properties if the integer
is assigned by insertion order.

Composes with: per-type intern dictionaries and value rendering.

Conflicts with: using either the dense integer or the hash for both semantic
and storage jobs.

Checks: `surrogate_semantic_ids_order_independent`,
`surrogate_dense_keys_order_dependent`,
`surrogate_tick_add_prints_value`.

Result: the content hash is semantic identity. The per-type dictionary maps it
to a dense integer. Ref columns store the integer. Logical equality and logs
use the value.

### Idea 9: hash child semantic identities, then store child dense integers

Reason: computing a parent hash from a child dense integer imports dictionary
insertion order into semantic identity.

Composes with: idea 8 and nested ref columns.

Conflicts with: hashing the physical row after dense refs have replaced
semantic refs.

Checks: `parent_semantic_hash_ignores_dense_mate`,
`parent_hash_from_dense_would_be_order_dependent`.

Result: lowering has two ordered forms. The semantic form contains child
hashes and computes the parent hash. The storage form substitutes the dense
mates after that computation.

### Idea 10: allow dense keys to change after collection

Reason: refcount collection removes the last dictionary row. Reusing its dense
integer can create stale aliasing, while never reusing it means reinterning the
same semantic value may receive a new dense integer.

Composes with: value support GC and value-rendered logs.

Conflicts with: any external contract that treats the dense integer as stable
semantic identity.

Check: `surrogate_reintern_changes_dense_not_semantic`.

Result: dense ids are storage-local and lifetime-local. The hash and rendered
value remain stable. Dense ids cannot appear in oracle output.

### Idea 11: preserve support counting through the mate dictionary

Reason: splitting identity and storage must retain the round-1 insert, share,
release behavior.

Composes with: idea 8 and the recovered policy-bundle tape.

Conflicts with: one dictionary row per referrer or permanent intern rows.

Check: `surrogate_share_release_refcounts`.

Result: equal semantic values share one dictionary row and one value row.
Support changes from 1 to 2 to 1 to 0. Logical deltas remain `+value`, empty,
empty, `-value`.

### Idea 12: place policy words in declarations

Reason: a type whose policy never varies can state identity, mutation, merge,
and lifetime once.

Composes with: policy-specific DDL.

Conflicts with: using one declared shape under both policies without another
explicit marker.

Checks: `coexistence_spellings_assign_same_policies`,
`coexistence_policy_token_counts`, `policy_specific_ddl_shapes`.

Result: the worked example uses four policy words. `route` is entity.
`body_page`, `body_redirect`, and `view` are value. This option emits one
physical policy layout per declared type.

### Idea 13: place policy words at body use sites

Reason: the same declared column shape may be used as entity in one producer
and value in another.

Composes with: policy-specific physical tables plus a logical rel view.

Conflicts with: one physical table per logical rel name and one-time policy
spelling.

Checks: `coexistence_spellings_assign_same_policies`,
`coexistence_policy_token_counts`.

Result: the shown worked example uses four policy words, one per producer
rule. A logical name admitted under both policies lowers to distinct value and
entity tables plus a union view.

### Idea 14: combine declaration and use-site placement

Reason: most declared types select one policy, while a smaller set of shared
shapes needs both. Repeating all choices at every producer adds source words.
Forcing all choices into declarations duplicates shared shapes.

Composes with: ideas 12 and 13.

Conflicts with: a default inside a dual-policy declaration.

Checks: `coexistence_spellings_assign_same_policies`,
`coexistence_policy_token_counts`.

Result: a declaration contains exactly one policy when fixed. A declaration
that lists `policy(value, entity)` requires a policy word at every producer.
The worked example uses six choice words because `route` admits both while
the three body/view types pin value. No listed policy is selected implicitly.

### Idea 15: restrict the deep value closure at an entity ref

Reason: an interned value containing an entity id keeps the same content hash
when that entity changes. A printer that recursively expands the entity would
change the rendered value without changing the value identity.

Composes with: entity ids as stable scalar values and entity-to-value refs.

Conflicts with: treating every ref column as a recursively expanded value
edge in both directions.

Checks: `value_to_entity_deep_render_can_change`,
`value_to_entity_opaque_text_stays_stable`,
`cross_policy_ref_modes_are_explicit`.

Result: value-to-value, entity-to-value, and entity-to-entity refs may be deep
match paths. Value-to-entity stores an identity ref and the value printer
stops at that id. A rule may join through the id explicitly.

### Idea 16: let an entity current row support immutable values

Reason: an entity commonly points at a versioned immutable body. Replacing
that ref should add support to the new value and release support from the old
value in the same tick.

Composes with: value refcount GC, entity keyed replacement, and idea 15.

Conflicts with: treating entity lifetime and value lifetime as one collector.

Check: `entity_ref_replacement_releases_old_value`.

Result: entity current rows count as value supports. Entity history rows do
not keep old values live unless retention policy explicitly asks them to.

### Idea 17: scope the cons-cell amendment to value lists

Reason: a content-addressed list needs fixed-arity keys, so cons cells state
the whole value recursively. An entity list has an extrinsic id and may change
arity while preserving identity.

Composes with: round-1 cons tail sharing and entity update.

Conflicts with: applying the cons requirement to all lists.

Checks: `cons_shares_tails_indexed_does_not`,
`entity_variable_arity_update_preserves_id`.

Result: value lists use cons or an out-of-band whole-sequence hash. Entity
lists may use indexed element rows and mutable history.

### Idea 18: scope the N-variant-table result to immutable variants

Reason: an immutable value never changes variant, so one row lives in exactly
one variant table. An entity may move from `page` to `redirect` while its id
and history remain continuous.

Composes with: variant rels, entity history, and keyed replacement.

Conflicts with: a derived current tag view as the only record of entity
variant history.

Check: `entity_enum_variant_change_preserves_id_and_history`.

Result: value enums retain N variant rels and a derived tag view. Entity enums
need a shared id, current tag, and tagged history; variant payload tables may
remain separate.

### Idea 19: keep logical match paths independent of identity policy

Reason: both physical layouts use integer ref columns. Policy changes
identity, update, and lifetime behavior rather than the number of logical
joins.

Composes with: round-1 depth 1, 2, and 3 SQL and ordinary edge rels.

Conflicts with: duplicating matcher syntax for value and entity.

Check: `match_path_cost_is_policy_independent`.

Result: the depth-3 worked path emits two joins under either policy. An entity
cycle requires a visited relation for recursive traversal; a value DAG does
not.

### Idea 20: separate automatic value death from explicit entity death

Reason: support equals reachability only for the content-addressed DAG.
Entity continuity and cycles deliberately violate that proof.

Composes with: ideas 5, 6, 11, and 16.

Conflicts with: the unqualified round-1 statement that domination dissolves
entirely into support counting.

Checks: `support_equals_reachability`,
`lifetime_claim_is_scoped_by_policy`,
`entity_cycle_explicit_set_retires`.

Result: domination dissolves into support counting on the value plane.
Entities pay current/history storage, retention decisions, explicit retire,
outside-ref checking, and atomic retirement sets.

### Idea 21: close the fixpoint only after a zero-finding replay

Reason: a productive round says more cases remain. The contract requires one
full attack round that adds nothing.

Composes with: every prior idea and all 66 checks.

Conflicts with: stopping after the third productive round.

Check: `fixpoint_stops_after_full_zero_finding_round`.

Result: round 4 replayed every row in the conclusion matrix below. No new
state, storage, tick, query, migration, recursive, or checker distinction was
found.

## Why the two policies disagree

| question | value answer | entity answer | reason for disagreement |
|---|---|---|---|
| what continues to exist | equal canonical content | one extrinsic id | equality and continuity answer different questions |
| what an update means | a new value and usually a new hash | a new current version of the same id | content identity includes fields; entity identity excludes mutable fields |
| equal content twice | one shared row with support 2 | two ids unless the caller reuses one | value sharing follows equality; entity sharing is an explicit id choice |
| when a row dies | support reaches zero | explicit retirement passes ref checks | value DAG reachability is computable locally; entity lifetime is an input event |
| cycles | impossible among deep values | permitted | value hashes require child hashes first; entity ids exist before refs close |
| variable arity | fixed-arity cons or whole-sequence hash | mutable indexed rows | the value key must include the sequence; the entity key is already the id |
| enum variant change | new value in another variant rel | same id with a new current tag and history row | immutable variants have no transition; entities do |
| deep ref to entity | identity scalar in the immutable value | normal traversable ref from an entity | recursive expansion through mutable state would change an interned value view |

Both answers coexist through an explicit policy position. Fixed types carry
one declaration word. Dual-policy shapes list both policies and require a
body use-site word. Omission fails resolution.

## Round 4 zero-finding matrix

| conclusion attacked again | value result | entity result | disposition |
|---|---|---|---|
| rel is the common construct | same value table shape plus intern columns | current plus history rels | reaffirmed |
| struct is one pinned bundle | explicit value bundle | explicit entity bundle | amended: two bundles, no default |
| enum uses variant rels | N immutable variant rels | current tag plus tagged history | amended by policy |
| list needs cons | yes for content keying | no for extrinsic identity | amended by policy |
| nesting is ref columns | dense value mates | dense entity ids | reaffirmed |
| four policy bits | content, immutable, support, set | extrinsic, history, retire, keyed | reaffirmed |
| merge state may rise while boundary retracts | set arrival stays additive | keyed update emits `-old,+new` | reaffirmed |
| cycles crack the model | crack value | represented by entity | amended at the common-construct level |
| support collection is complete | complete on the value DAG | not used as entity lifetime | amended by scope |
| SQL cascade is wrong for sharing | still wrong | retirement needs explicit ref checks | reaffirmed |
| dense int conflicts with content identity | dense mate beside hash | extrinsic dense id already semantic | amended by split columns |
| tick logs must print values | hash and dense id omitted | id plus current value printed | reaffirmed |
| match depth equals join count plus one | unchanged | unchanged | reaffirmed |
| edge tables are ordinary rels | yes | yes | reaffirmed |
| typed fields use refs | yes | yes, with explicit policy metadata | reaffirmed |
| untyped json remains inline | unchanged | unchanged | reaffirmed |
| recursive traversal terminates by DAG | yes | visited relation required | reaffirmed with entity cost |
| checker inference stays in prolog | yes | yes | reaffirmed |
| checked type graph is published as rel rows | yes | yes | reaffirmed |
| intern scope is per type | yes | no intern dictionary for entity | amended by policy |

## Path to the final suggested arrangement

1. Keep `rel` as the only storage and query construct.
2. Require `value` or `entity` at a fixed declaration.
3. Permit `policy(value, entity)` only as an explicit dual-policy declaration.
4. Require a body use-site word for every producer of a dual-policy rel.
5. Lower value identity in two phases: canonical content with child hashes,
   then dense-mate substitution for stored refs.
6. Lower entity state into a keyed current table and an append-only history
   table.
7. Count entity current refs as supports for value rows.
8. Stop value rendering at an entity identity ref.
9. Collect values at support zero. Retire entities through explicit checked
   sets.
10. Print logical values in tick logs and keep dense intern rows outside the
    boundary.

The coexistence ranking in the verdict uses this arrangement: hybrid first,
declaration-only second, use-site-only third.
