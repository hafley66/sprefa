# List flavor template lab

## Context

`0_generic_expand.pl` supplied a declaration-only artifact path and a stable
canonical-name function. This lab exercises that path with one existing
`list(text)` standin and three additional term-door-only constructors. No
parser or runtime storage lowering is changed.

## Candidate matrix

| ordering | identity | duplicates | coherent | reason when incoherent |
|---|---|---|---:|---|
| dense | owned | sequence | yes | |
| dense | entity | sequence | yes | |
| dense | interned content | sequence | yes | positions distinguish repeated values |
| linked | owned | sequence | yes | |
| linked | entity | sequence | yes | |
| linked | interned content | sequence | yes | member occurrences retain order |
| unordered | owned | sequence | no | a sequence requires a total order |
| unordered | entity | sequence | no | a sequence requires a total order |
| unordered | interned content | sequence | no | a sequence requires a total order |
| dense | owned | set | no | dense positions have no set ordering contract |
| dense | entity | set | no | dense positions have no set ordering contract |
| dense | interned content | set | no | dense positions have no set ordering contract |
| linked | owned | set | no | predecessor links have no set ordering contract |
| linked | entity | set | no | predecessor links have no set ordering contract |
| linked | interned content | set | no | predecessor links have no set ordering contract |
| unordered | owned | set | yes | |
| unordered | entity | set | yes | |
| unordered | interned content | set | yes | |

## Decisions

The lab contains these template instances:

| constructor | ordering | identity | duplicates | declarations |
|---|---|---|---|---|
| `list(text)` | dense | owned | sequence | entity plus member |
| `list_entity_dense_sequence(text)` | dense | entity | sequence | entity, member, owner junction, refcount |
| `list_interned_set(text)` | unordered | interned content | set | content entity, value dictionary, membership |
| `list_entity_linked_sequence(text)` | linked | entity | sequence | entity, member, predecessor link |

All artifacts are declarations. The interned value dictionary keeps text out of
the member key. Artifact names are derived from the canonical type name plus a
fixed suffix.

## Evidence

| constructor | canonical minted list relation |
|---|---|
| `list(text)` | `__gen__list_text_df210f232c1299bd` |
| `list_entity_dense_sequence(text)` | `__gen__list_entity_dense_sequence_text_42382f22da23f5c6` |
| `list_interned_set(text)` | `__gen__list_interned_set_text_5de2cb6bdb4dd03b` |
| `list_entity_linked_sequence(text)` | `__gen__list_entity_linked_sequence_text_9e34f8b0a209ed35` |

The fixture file contains arrivals and retractions across 5, 3, 3, and 3
ticks respectively. The `list_entity_dense_sequence` fixture is reversed in a
plunit expansion test and produces byte-identical expanded terms.

| constructor | shared template lines | flavor-specific artifact lines |
|---|---:|---:|
| `list(text)` | 56 | 9 |
| `list_entity_dense_sequence(text)` | 56 | 16 |
| `list_interned_set(text)` | 56 | 12 |
| `list_entity_linked_sequence(text)` | 56 | 13 |

The shared count covers generic instance discovery, artifact lowering,
canonical naming, collision checks, replacement, and suffix naming. Specific
counts are the `list_flavor_artifacts/2` clauses including their artifact
literals.

## Verification

`swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g "run_tests(expansion_order),halt"`
passes 27 tests. `swipl -q -l v6/prolog/conformance/go.pl -g go -g halt`
passes all fixtures, including the four list-flavor fixtures.

## Staffing

Implementation: terra lane, worktree yes. Base SHA: `7e477da1`.

