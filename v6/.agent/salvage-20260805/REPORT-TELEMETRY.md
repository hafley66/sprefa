# REPORT

## TOC
- What and where
- Deliverable inventory
- The standard envelope
- Emitter diffs
- Reader: perf-n1
- Gates (receipts)
- Deviations
- Next action

## What and where

The telemetry lane: standardize every JSONL telemetry line in the repo on one
envelope (`actor`/`seam`/`unit`/`tick`/`rows`/`ms`), added additively across the
dl and tsv2 emitters, and teach the perf-n1 reader to fold the standard envelope
as a fourth shape. Field names could converge because the same problems kept
being re-solved, so the additions are aliases onto names the emitters already
carried.

Location: spec at `v6/TELEMETRY.md`; the three emitters under
`v6/dl/src/0_trace.ts`, `v6/tsv2/serve/0_trace.ts`, `v6/tsv2/runtime/trace.ts`;
reader at `v6/tools/perf-n1.mjs`.

## Deliverable inventory

| file | role |
|---|---|
| `v6/TELEMETRY.md` | envelope spec, emitter inventory, legacy alias table, UNBUILT rows |
| `v6/dl/src/0_trace.ts` | dl actor/seam/unit additive fields |
| `v6/tsv2/serve/0_trace.ts` | serve tick line actor/seam |
| `v6/tsv2/runtime/trace.ts` | runtime rule record actor/seam/unit/ms |
| `v6/tsv2/runtime/types.ts` | `IServeRuleEvent` widened with the standard fields |
| `v6/tools/perf-n1.mjs` | fourth accepted shape, the standard envelope |
| `v6/tools/perf-n1.test.mjs` | new tests with a fixture line per emitter |

## The standard envelope

```
ts     epoch ms (pino `time` serves it)
actor  who    package.component: "dl.runtime", "tsv2.serve", "tsv2.runtime"
seam   where  sql | effect | bind | ingest | tick | host
unit   why    rule id, rel name, host name, or normalized sql
tick   logical tick when in scope
rows   row count when in scope
ms     wall duration when in scope
```

Additive only. A field is left absent where the idea is out of scope (an
aggregate tick line has no single `unit`; an EDB statement has no `tick`).

## Emitter diffs

| emitter | line | added |
|---|---|---|
| dl | tick line | `actor="dl.runtime"`, `seam="tick"`, `ms` (= `wall_ms`) |
| dl | EDB statement | `actor="dl.runtime"`, `unit=normalizeSql(sql)` |
| tsv2 runtime | rule record | `actor="tsv2.runtime"`, `seam="sql"`, `unit` (= `rule`), `ms` (= `wall_ms`) |
| tsv2 serve | tick line | `actor="tsv2.serve"`, `seam="tick"` |

## Reader: perf-n1

Fourth accepted shape: any line with `actor` and `seam`. For it, unit comes
straight off the line, one statement per line, `rows`/`ms` summed. The existing
serve/dl/edb branches still fire first, so the added standard fields never
reclassify those lines.

## Gates (receipts)

```
cd v6/dl && pnpm install && pnpm test
  tests 99  pass 98  fail 0  skipped 1

cd ../tsv2 && pnpm install && pnpm test
  tests 146  pass 145  fail 0  skipped 1

node --test v6/tools/perf-n1.test.mjs
  tests 7  pass 7  fail 0
```

`pnpm typecheck` is clean in tsv2. The dl typecheck error at
`tests/4_hosts.test.ts:482` is present on the base commit (verified by stash) and
is out of this lane's files.

## Deviations

1. Effect channel events in tsv2 stay byte-pinned to the registry.pl schema by
   `tests/traceSchema.test.ts`; the standard fields go on the JSONL line, not the
   channel event. Recorded under Constraint notes in TELEMETRY.md.
2. dl bind/effect/ingest record shapes stay as-is (`tests/0_trace.test.ts`
   `deepEqual`); the standard names are documented as aliases rather than
   renamed.

## Next action

Wire the rust and prolog emitters (UNBUILT rows in TELEMETRY.md): map the
`tracing` JSON layer and `format/3` to the same envelope.
