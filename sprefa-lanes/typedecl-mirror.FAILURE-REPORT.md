# Mirror Fix Failure Report

## Blocker

The required strict converter gap count did not change. The compiler mirror fix
passes the direct repro, conformance, TEXT_DOOR, and focused expansion tests.
The converter's strict fallback still rewrites the affected columns before the
compiler receives them.

## Exact command

```text
cd /Users/chrishafley/projects/sprefa/.boop-worktrees/fix/typedecl-mirror/v6/tsv2 && npx tsx scripts/openapi_roundtrip_check.ts
```

## Exact result

```text
compile (compile_dl6.sh) exit code: 0
emit-back (4_emit_jsonschema / 5_emit_openapi): OK
ROUNDTRIP PASS: componentName:212 propName:786 kind:769/0/17 refTarget:256/0/0 nullable:771/0/15
```

```text
Converter strict-mode dropped columns (G1) + nullable-array (G2): 75 + 4
```

## Throw site

```text
v6/tsv2/scripts/openapi_to_dl6.ts:276-278
v6/prolog/0_type_plane.pl:128
```

`applyStrictFalls/1` unconditionally rewrites generic columns on ref targets
to `json`; the compiler cannot observe those original `option/list` types.
