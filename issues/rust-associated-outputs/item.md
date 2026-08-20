---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: terra
status: done
priority: normal
epic: relational-type-schema
labels:
- area:dl6
- intent:emitter
blocked_by: ['@compiler-type-relations', '@anonymous-product-values']
lane: emitters
lane_seq: 0
collision: [type-emitters, catalog-schema]
closed: 2026-08-18
---

# Emit Rust traits and associated outputs

## Description

Lower type_relation IR to Rust traits: consume Self implicitly, emit generic inputs, project one product return into named associated types, and define evidence-row impl emission. Compile generated artifacts.

## Acceptance Criteria

- [x] `Self` is consumed as the implicit trait implementer and never emitted as a field.
- [x] Non-key type inputs emit Rust trait generic parameters.
- [x] Keyed inputs plus return express the declared functional relationship.
- [x] Product return fields emit named associated type items through semantic roles.
- [x] Evidence rows have a documented `impl` emission contract.
- [x] Generated scalar and multi-associated-output Rust compiles and executes focused fixtures.

## Tests Run

## Implementation Notes

Mapping: `rel convert(Self:key(type), Input:key(type)) -> Output` emits `trait Convert<Input> { type Output; }`. A zero-input relation emits `trait Name { type Output; }`. A product return emits one associated type per ordered product field, using Rust type-name normalization and refusing post-normalization duplicates as `associated_output_name_collision(Relation,Name)`. Missing return and nonfunctional selector keys receive `associated_output_missing_return/1` and `associated_output_nonfunctional/1`. Evidence rows emit `impl Trait<Inputs> for SelfType` only when the compiler closure contains one complete keyed return row; conflicting rows are rejected earlier. Do not infer associated outputs from observed implementation counts.

## Comments

### 2026-08-19T01:56:41Z · @codex

Integrated as fcdb7053d. CI: type_relation_ir 38/38; authored DL6 renderer output compiled with rustc; same-local-name module identity and evidence ambiguity tests pass.
