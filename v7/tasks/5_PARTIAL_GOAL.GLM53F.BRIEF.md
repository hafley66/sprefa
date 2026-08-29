# Prove userland Partial over the DL7 type graph

## Description

Compile one source file together with `v7/prelude/0_types.dl7`. The prelude
defines `Partial` and `Option` as ordinary relations. `cons/3` builds the
ordered constructor-argument list and `intern/3` returns the canonical
`application(Constructor, Arguments)` identity. Userland rules classify the
Partial result and derive its transformed edges.

## Signatures

```prolog
compile_dl7(+Path, -CompilerRows, -RuntimeProgram, -Diagnostics).
compile_unit(+Unit, -CompiledUnit, -Diagnostics).

% CompiledUnit = compiled_unit(TypeGraphFacts,
%                              RuntimeProgram,
%                              CompilerFacts).
```

## Timeline

```text
prelude + source
    -> reader
    -> checked graph and positive rules
    -> graph rows become evaluator seeds
    -> shared libtime closure
    -> type graph facts + retained runtime program + compiler facts
```

## Storage and uniqueness

- Constructor identity plus its ordered argument list keys each application.
- Repeating the same application produces the same structural identity.
- Partial copies each source edge label and ordinal.
- Each copied target is the canonical Option application of the source target.
- The source graph remains present beside the generated graph.

## Acceptance Criteria

- [ ] Partial and Option behavior exists only in the DL7 prelude rules.
- [ ] `cons/3` and `intern/3` are phase-independent kernel relations.
- [ ] Partial applications emit ordinary `node/1` and `product/1` facts.
- [ ] Partial copies source labels and indices and maps targets through Option.
- [ ] Compiler closure and runtime program retain the same checked call shape.
- [ ] Compiling twice in one SWI process produces identical terms.
- [ ] Compiler code lives in `v7/src/2_comptime/1_type_compiler.pl`.
- [ ] No DL6, Rust, TypeScript, effect, tick, or emitter dependency is added.
- [ ] No standalone test file is added.

## Tests Run

- [ ] One direct SWI receipt proves `Partial(User)`, two mapped edges, canonical
      Option targets, and identical repeated compilation.

## Implementation Notes

The source requests a compile-known Partial application with an ordinary
ground fact. All type transformation logic remains authored in `.dl7`.
