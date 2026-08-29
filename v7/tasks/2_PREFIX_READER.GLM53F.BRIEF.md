# Read the bounded DL7 prefix surface

## Description

Read `.dl7` characters before SWI applies Prolog capitalization rules. Bare
identifiers remain names at every capitalization. `?Name` creates a logical
variable identity shared within one top-level form. Quoted symbol data,
strings, integers, comments, nested forms, spans, and deterministic
diagnostics use the same reader path for files and SWI quasi quotations.

## Signature

```prolog
read_dl7(+Path, +Text, -Forms, -SourceRows, -Diagnostics).
```

## Acceptance Criteria

- [ ] PascalCase and lowercase identifiers both read as `atom(Name)`.
- [ ] `?Name` identities share within one top-level form and `?_` stays fresh.
- [ ] Prefix forms, literals, comments, escapes, and complete spans are retained.
- [ ] Files and quasi quotations share one text-to-unit pipeline.
- [ ] Reader modules import no DL6 declaration or statement dispatcher.
- [ ] Production code lives in `v7/src/0_reader/` in dependency order.
- [ ] Existing tests remain consolidated under `v7/test/`.

## Tests Run

- [ ] The six reader and entrypoint tests pass in one focused SWI command.

## Implementation Notes

The reader emits ground compiler data with explicit variable identities. SWI
variables are introduced later by `v7/src/1_libtime/0_evaluator.pl`.
