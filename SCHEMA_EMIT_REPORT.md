# Schema emit lane report

## Result

The prior rc=1 was the TypeScript typecheck gate. It reports an existing type
error in the setup symlink target
`/Users/chrishafley/projects/sprefa/v6/tsv2/gen_emitted/golden-flex.ts:3531`:

```text
gen_emitted/golden-flex.ts(3531,3): error TS2322: Type 'Observable<unknown>' is not assignable to type 'Observable<ITickDeltas>'.
```

The generated file is outside this worktree and outside the owned source
surface. The OpenAPI implementation and all owned tests pass. The symlink
`v6/tsv2/gen_emitted` remains an uncommitted setup artifact.

## Deliverables

- JSON Schema draft 2020-12 emitter from catalog declaration rows.
- OpenAPI 3.1 emitter sharing the JSON Schema relation-shape builder.
- Additive per-fixture `.schema.json` sweep artifacts.
- `GET /openapi.json`, rebuilt on program load and program swap.
- Prolog fixture tests and ephemeral HTTP tests.
- Redocly validation configuration and checked-in OpenAPI fixture.

## Gate outputs

```text
SWEEP total=347 compiled=247 unsupported=100 crash=0
RUN total=247 identical=246 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
FINAL total=247 final_identical=246 final_wrong=0 no_oracle_final=1
MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0 (informational)
```

```text
just conformance
PASS for the full conformance corpus
```

```text
swipl -g run_tests -t halt compile/test/plunit_tests.pl
% All 559 (+46 sub-tests) tests passed in 6.056 seconds (5.899 cpu)
```

```text
just tsv2-test
tests 198
pass 196
fail 0
skipped 2
```

```text
node --test --experimental-transform-types tests/serveOpenapi.test.ts
tests 3
pass 3
fail 0
```

```text
npx @redocly/cli lint ../prolog/compile/test/emit/openapi/struct_column_renders_canonical_json.openapi.json --config ../prolog/compile/test/emit/redocly.yaml
validated in 9ms
exit 0
warning: operation-4xx-response for GET /openapi.json, which only answers 200
```

```text
just typecheck
exit 1
gen_emitted/golden-flex.ts(3531,3): error TS2322: Type 'Observable<unknown>' is not assignable to type 'Observable<ITickDeltas>'.
```

## Commits

### `fb92909fe4ac16700b88748b260bb5d2eed39d3a`

`prolog: emit json-schema per fixture from catalog rows`

The complete file list is the output of:

```text
git show --format= --name-only fb92909fe4ac16700b88748b260bb5d2eed39d3a
```

It contains `v6/prolog/compile/4_emit_jsonschema.pl`,
`v6/prolog/compile/5_emit_openapi.pl`, `v6/prolog/sweep.pl`,
`v6/prolog/compile/test/plunit_tests.pl`, the OpenAPI and JSON Schema test
fixtures, `redocly.yaml`, and every generated file under
`v6/prolog/compile/out/*.schema.json`.

### Second commit

`tsv2: GET /openapi.json from the loaded program`

```text
v6/tsv2/serve/4_http.ts
v6/tsv2/serve/openapiDoc.ts
v6/tsv2/tests/serveOpenapi.test.ts
```
