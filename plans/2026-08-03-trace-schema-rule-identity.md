# Trace schema + rule identity through the compile seam

Status: PROPOSED, awaiting go/no-go. Base sha `e3a103ce`.

## What was asked

Lower conventional target-language trace statements, using a generic structured
logging shape under one env flag, so a Rust emitter can produce the same bytes.

## What already exists (receipts)

- `v6/tsv2/serve/0_trace.ts` — the TS spine. `node:diagnostics_channel` with
  four channels (`sprefa:tick|effect|bind|watch`) into **pino 10.3.1**
  (`tsv2/package.json:26`), installed only when `DL_PERF_LOG` names a path,
  `base: null, timestamp: false`, one aggregated JSONL line per tick.
- `v6/prolog/6_profile.pl` — the Prolog spine, same env flag, one JSON object
  per compile phase, opt-in the same way (`6_profile.pl:30`).
- `6_profile.pl:7-9` states the contract in prose: *lower snake case, elapsed
  values in `*_ms`, one aggregate record per measured unit rather than per
  predicate call*.
- `v6/prolog/ARCH.pl:819` (task `prolog_compile_profiling`, done) states the
  reason: *"JSONL under DL_PERF_LOG matching the runtime shape so numbers
  survive the rust flip"*.
- Zero `console.*` in `tsv2/runtime`, `tsv2/serve`, `tsv2/cli`.

So the flag, the sink, and the intent are landed. Two things are not.

## The two gaps

### Gap 1: the TS side does not follow the convention it named

| field | `6_profile.pl:97` | `runtime/types.ts:603` |
|---|---|---|
| elapsed | `wall_ms`, `cpu_ms`, `gc_ms` | `ms` (`IServeTickEvent.ms`, `IServeEffectEvent.ms`) |
| case | `gc_left_bytes`, `table_answers` | `witnessDigest` (`IServeEffectEvent`) |

Consumers that read the JSONL and would move with a rename:
`tsv2/scripts/memory-soak.ts:260`, `tsv2/scripts/crawl-bench.sh:294`,
`tsv2/tests/serveLeak.test.ts:50`, `tsv2/CRAWL-BENCH.md:159`.

### Gap 2: nothing crosses the compile seam

Emitted modules are plan data, not code with log lines in it
(`tsv2/gen_emitted/golden-flex.ts:2001-2005` hands statement arrays to
`IncrementalRuntime`). The statement interfaces carry no identity:

```ts
// runtime/types.ts:118
export interface IIncrementalEdgeStatement {
  readonly headRel: string;         // "pickable"
  readonly headKind: "log" | "set";
  readonly projectSql: string;
  // no rule identity, no source location
}
```

So the trace can say *"tick 7: 4 rels, 312 rows, 18 statements, 41ms"* and can
never say **which rule** fired. That is the whole ask.

## Build vs buy

No logger is written. Per target:

| target | choice | why |
|---|---|---|
| TypeScript | `node:diagnostics_channel` + **pino** | already the dependency and already the seam. diagnostics_channel is the platform's own publish seam, so the off-path cost is one `hasSubscribers` branch. |
| Prolog | `library(http/json)` `json_write_dict/3` | already in `6_profile.pl:116`. |
| Rust (future) | **`tracing` + `tracing-subscriber`'s `json` layer** | the ecosystem default, structured fields are first class, and a `json` formatter emits one object per event. Field ORDER is not guaranteed stable across versions, so the byte receipt below must sort keys or the Rust layer must be a small custom `FormatEvent` impl over `tracing-subscriber`. That is a formatter, not a logger. |
| Rust alternatives considered | `slog` — structured, mature, but the ecosystem moved to `tracing` and it needs its own macro discipline at every call site. `log` + `env_logger` — unstructured strings, wrong shape for a byte contract. `fastrace` — fast, distributed-trace shaped (spans/parents), heavier than a per-tick record needs. | |

## Design

### Layer 1: the schema is data, not prose

The convention currently lives in a comment. Make it a fact table both emitters
read, in the repo's existing idiom (`compile/registry.pl` is the precedent).

```prolog
% v6/prolog/trace_schema.pl
%
% trace_event(Name, Fields) -- Fields is an ordered list of
%   field(Key, Type, Class) where Class = stable | timing.
%
% Key ordering IS the wire ordering. A `timing` field is excluded from the
% cross-target byte receipt (a wall clock cannot be byte-identical); a `stable`
% field must be reproducible by any emitter on the same program and schedule.
trace_event(tick, [ field(tick,       int,  stable),
                    field(rels,       int,  stable),
                    field(rows,       int,  stable),
                    field(statements, int,  stable),
                    field(rules,      list, stable),
                    field(wall_ms,    real, timing) ]).
trace_event(rule, [ field(rule,       text, stable),
                    field(rows,       int,  stable),
                    field(wall_ms,    real, timing) ]).
% effect / bind / watch: as today, with witnessDigest -> witness_digest
%                        and ms -> wall_ms.
```

Rail: every published key matches `^[a-z][a-z0-9_]*$`, every `timing` field ends
`_ms`. Fails the build, not a review.

### Layer 2: rule identity

**Signature (Prolog, `lower.pl`):**

```prolog
% rule_id(+Module, +HeadRef, +Ordinal, -RuleId)
%   RuleId is "<module>:<name>/<arity>#<ordinal>", Ordinal 1-based among the
%   LOWERED rules sharing that head ref, in lowering order.
%
%   Stable under edits elsewhere in the file. Changes when two rules with the
%   same head are reordered, which is the honest answer: they are two
%   interchangeable clauses of one relation and only their order tells them
%   apart.
```

**Signature (TS, `runtime/types.ts`):**

```ts
export interface IIncrementalEdgeStatement {
  readonly headRel: string;
  readonly ruleId: string;          // NEW: "golden-flex:pickable/3#2"
  // ... unchanged
}
export interface IIncrementalLevelStatement {
  readonly headRel: string;
  readonly ruleId: string;          // NEW
  // ... unchanged
}

export interface IServeRuleEvent {
  readonly rule: string;
  readonly rows: number;
  readonly wall_ms: number;
}
```

**Where the event is produced.** `IncrementalRuntime.applyEdges`
(`runtime/1_incremental.ts:766`) and `applyLevelsBeforeEdges` /
`applyLevelsAfterEdges` (`:778`, `:876`) already iterate the statement arrays and
already know each statement's changed-row count.

```ts
// pseudo-code, inside the existing per-statement step
//   if (!ruleChannel.hasSubscribers) -> unchanged path, one branch
//   else publish({ rule: statement.ruleId, rows: changed, wall_ms: since })
// The subscriber accumulates into pendingRules, exactly as pendingEffects
// works today (serve/0_trace.ts:39-47), and the tick line drains it.
```

**Instance lifetimes.**

| holder | lifetime | reset |
|---|---|---|
| the four (now five) channels | process | never; module scope |
| `logger` (pino destination) | process, installed once | `installed` guard, `serve/0_trace.ts:35` |
| `pendingRules` | one tick | drained into the tick line, same as `pendingEffects` |
| `ruleId` on a statement | one loaded program | replaced wholesale on program swap |

**Where the trace spine lives.** Today it is `serve/0_trace.ts`, so a runtime
used WITHOUT serve (`scripts/run-emitted.ts`, the tests) cannot publish. The
channels and publishers move to `runtime/trace.ts`; the pino sink installation
stays app-side. A library publishes; an application chooses the sink. Note
`tests/serveLeak.test.ts:138` names pino's file handle, so that test moves with
it.

### Layer 3: the byte receipt

The artifact a Rust emitter is graded against:

```
DL_PERF_LOG=$WORK/trace.jsonl  <run golden-flex through the served engine>
jq -c 'del(.wall_ms) | (.rules[]? |= del(.wall_ms))' $WORK/trace.jsonl
  | diff -u v6/tsv2/goldens/golden-flex.trace.jsonl -
```

Stable fields only. A second emitter in any language is correct when this diff
is empty. This is the only sentence in the plan that a Rust lane actually needs.

## Scope, in landing order

| slice | touches | gradeable by |
|---|---|---|
| A. schema as data + rename `ms`->`wall_ms`, `witnessDigest`->`witness_digest` | `trace_schema.pl` (new), `runtime/types.ts`, `serve/0_trace.ts`, 4 consumers | the naming rail; existing soak legs still read their fields |
| B. `ruleId` through the seam | `lower.pl`, `runtime/types.ts`, `runtime/trace.ts` (moved), `1_incremental.ts`, all 197 tracked `gen_emitted/*.ts` regenerate | text-door receipt still 196/196 byte-identical (both doors regenerate together); tick-log goldens unaffected, trace is a separate stream |
| C. the pinned trace golden | `v6/tsv2/goldens/golden-flex.trace.jsonl` (new), one script | the diff above |

## Deferred, and named

**Source spans** (`span: { line, col }` per rule). `compile/parse_dl.pl:185-196`
already has the line table and `:230-240` resolves a statement to line/column,
but it resolves by RELATION REFERENCE, so two rules with one head collapse to
the first. Per-rule spans need the origin threaded through `1_expansion.pl` and
`1_host_expand.pl`, since neither a match-expanded nor a host-expanded rule is
the rule the user wrote. That is the `codex/rel-ref-file-span-lab` branch's
subject, not this plan's.

## Open questions

1. Renaming `ms` -> `wall_ms` breaks anyone reading today's JSONL. Four
   in-repo consumers move with it; nothing outside the repo is known to read it.
   Confirm nothing external does.
2. `rules: []` on every tick line grows the log by one entry per firing
   statement. At golden-flex `many` that is 18 statements x 10 ticks. Fine here;
   state the ceiling before pointing it at a real corpus.
3. Whether the tick line should carry `rules` at all when the array is empty, or
   omit the key. Omitting is smaller; carrying it always is one less branch in
   every reader, including the Rust one.
