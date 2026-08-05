# TELEMETRY

One envelope, one JSONL log. Every JSONL telemetry line in this repo converges on the same field names.

## TOC
- The standard envelope
- Mapping the standard fields onto `registry.pl`'s `trace_event/2`
- Emitter inventory
- What was added per site
- Legacy aliases
- Constraint notes
- UNBUILT next steps
- Reader: perf-n1

## The standard envelope

```
ts     when   epoch ms (pino `time` serves this; pino's field is kept as-is)
actor  who    package.component, e.g. "dl.runtime", "tsv2.serve", "tsv2.runtime"
seam   where  one of: sql | effect | bind | ingest | tick | host
unit   why    the rule id, rel name, host name, or normalized sql the line is about
tick   when   logical tick when in scope, absent otherwise
rows   what   row count when in scope
ms     what   wall duration when in scope
```

Every field is additive. Existing fields stay; the standard name is added
alongside the old one rather than breaking readers. A field "when in scope" is
left absent on a line where that idea does not apply (an aggregate tick line has
no single `unit`; an EDB statement has no `tick`; a bind firing has no `ms`).

## Mapping the standard fields onto `registry.pl`'s `trace_event/2`

The tsv2 serve layer's wire keys come from `v6/prolog/registry.pl`'s
`trace_event/2` table via `runtime/0_traceSchema.ts`. The standard envelope is a
thinner orthogonal index over those records:

| standard | trace_event/2 key | note |
|---|---|---|
| `actor` | (new) | package.component, not in registry.pl |
| `seam` | (new) | where the event happened |
| `unit` | `rule`, `rel`, `host`, `path` | the thing each record is about |
| `tick` | `tick` | same name |
| `rows` | `rows` | same name |
| `ms` | `wall_ms` | standard name for the same clock |

`ts` is served by pino's own `time` field on the dl emitter; the tsv2 serve
sink sets `timestamp: false`, so it carries no `time` and adds no `ts` either.

## Emitter inventory

| emitter file | event | current fields | standard fields added |
|---|---|---|---|
| `v6/dl/src/0_trace.ts` | tick line (`flushTick`) | `tick, wall_ms, stmt_count, stmt_ms_total, stmt_ms_max, effects[], binds[], ingest, rss_kb` | `actor="dl.runtime"`, `seam="tick"`, `ms` (= `wall_ms`) |
| `v6/dl/src/0_trace.ts` | EDB statement line (`onSqlMessage`) | `seam="edb", sql, ms` | `actor="dl.runtime"`, `unit=normalizeSql(sql)` |
| `v6/tsv2/runtime/trace.ts` | rule record (`ruleChannel.publish`) | `rule, rows, wall_ms` | `actor="tsv2.runtime"`, `seam="sql"`, `unit` (= `rule`), `ms` (= `wall_ms`) |
| `v6/tsv2/serve/0_trace.ts` | tick line (`logger.info`) | `tick, rels, rows, statements, wall_ms, rules[], effects[], binds[], watches[]` | `actor="tsv2.serve"`, `seam="tick"` |

## What was added per site

Each JSONL emit site in the three owned files now converges on `actor` (who) and
`seam` (where), plus `unit`, `ms`, `rows`, `tick` where the line is about a
single unit and the value is known.

- dl tick line: aggregate, so `unit` and `rows` are not in scope; `ms` is added
  as the standard name for `wall_ms`.
- dl EDB line: `unit` is the normalized SQL (literals collapsed to `?`), keeping
  the same shape perf-n1 already folds its EDB census around.
- tsv2 runtime rule record: `unit` renames `rule`, `ms` renames `wall_ms`.
- tsv2 tick line: aggregate, `unit`/`rows`/`ms` not added; `rows` and `wall_ms`
  already carry the ideas under the schema's names.

## Legacy aliases

Standard name `unit` and `ms` have different spellings on prior emit sites. The
standard field is ADDED alongside the old one; old readers are not broken.

| standard | legacy spellings at sites | site |
|---|---|---|
| `unit` | `rule` | tsv2 runtime rule record |
| `unit` | `rel` | dl `PerfBindEntry.binds[]`, tsv2 bind/watch records |
| `unit` | `host` | dl `PerfEffectEntry.effects[]`, tsv2 effect record |
| `unit` | `path` | dl `PerfIngestEntry.ingest` |
| `ms` | `wall_ms` | dl tick line, tsv2 rule/tick/effect records |
| `seam` | `seam` on EDB lines only | dl EDB line (already standard) |

## Constraint notes

1. tsv2 `sprefa:tick` and `sprefa:effect` CHANNEL events are pinned to exactly
   the `trace_event/2` keys by `v6/tsv2/tests/traceSchema.test.ts` ("exactly the
   schema's keys, in order"). Standard fields for those two are added at the
   JSONL LINE, never on the channel event.
2. dl `PerfBindEntry`/`PerfEffectEntry`/`PerfIngestEntry` record shapes are
   pinned by `v6/dl/tests/0_trace.test.ts` (`deepEqual`). The standard names are
   kept as documented aliases (above), not renamed.
3. The tsv2 serve nested effect/bind/watch records keep the registry.pl schema
   shapes. They are the wire contract a second emitter reproduces; the standard
   envelope is the overlay index, not a replacement.

## UNBUILT next steps

- v5 rust: map the `tracing` JSON layer onto the standard envelope (emit `actor`,
  `seam`, `unit`, `ms`, `rows`, `tick` alongside the registry.pl keys).
- prolog: `format/3` to JSONL on stderr, one JSON object per event, using the
  standard envelope field names.

## Reader: perf-n1

`v6/tools/perf-n1.mjs` accepts the standard envelope as a fourth shape: any line
carrying `actor` and `seam`. For those lines it sets `unit` from the line's
`unit`, counts one statement per line, and sums `rows`/`ms`. Covered by
`v6/tools/perf-n1.test.mjs` with a fixture line per emitter.
