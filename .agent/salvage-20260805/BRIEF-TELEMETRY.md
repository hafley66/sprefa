# Lane: telemetry — one envelope, one log, who/what/when/where/why

Deviate from this brief = STOP, write STOP.md, no further work.

## First action
```bash
git merge --ff-only 33b52887 && git rev-parse HEAD   # must be 33b52887; else STOP.md
```

## Ownership
- `v6/TELEMETRY.md` (new)
- `v6/dl/src/0_trace.ts` (field alignment only)
- `v6/tsv2/serve/0_trace.ts` + `v6/tsv2/runtime/trace.ts` (field alignment only)
- `v6/tools/perf-n1.mjs` + `v6/tools/perf-n1.test.mjs` (accept the envelope)
- `REPORT.md`, PR.

## The standard envelope (fixed, do not redesign)
Every JSONL telemetry line in this repo converges on these field names:
```
ts     when   epoch ms (pino `time` already serves this; keep pino's field)
actor  who    package.component, e.g. "dl.runtime", "tsv2.serve", "extract.cli"
seam   where  one of: sql | effect | bind | ingest | tick | host
unit   why    the rule id, rel name, host name, or normalized sql the line is about
tick   when   logical tick when in scope, absent otherwise
rows   what   row count when in scope
ms     what   wall duration when in scope
```
Existing extra fields stay; this is additive renaming/adding, never removal of
data. Where a current emitter uses a different name for one of these ideas
(e.g. `rule` for unit, `rel` for unit, `wall_ms` for ms), ADD the standard field
alongside the old one rather than breaking readers, and note each site in
TELEMETRY.md under "legacy aliases".

## Tasks
1. Inventory: read `v6/dl/src/0_trace.ts`, `v6/tsv2/serve/0_trace.ts`,
   `v6/tsv2/runtime/trace.ts`. TELEMETRY.md gets a table: emitter file, event,
   current fields, standard fields added.
2. Add the standard fields (`actor`, `seam`, `unit`, `ms`, `rows`, `tick` where
   known) to every JSONL publish/log site in those files.
3. `perf-n1.mjs`: a fourth accepted shape, the standard envelope (any line with
   `actor` and `seam`): unit = its `unit`, statements = 1 per line, rows/ms
   summed. One new test with a fixture line per emitter.
4. TELEMETRY.md also records the rust and prolog rows as UNBUILT next steps
   (v5 rust = tracing JSON layer mapping; prolog = format/3 to JSONL on stderr),
   one line each, no implementation.

## Validation
```bash
cd v6/dl && pnpm install && pnpm test
cd ../tsv2 && pnpm install && pnpm test
node --test v6/tools/perf-n1.test.mjs
```
All green. pnpm ONLY, never npm.

## PR flow (mandatory finish)
```bash
git add -A ':!BRIEF.md' && git commit -m "xtelemetry: standard telemetry envelope across dl/tsv2 emitters + perf-n1 reader"
git push -u origin lab/telemetry
gh pr create --title "xtelemetry: one telemetry envelope (actor/seam/unit/tick/rows/ms)" \
  --body "See v6/TELEMETRY.md. Additive field standardization; legacy aliases preserved. REPORT.md has receipts."
```

## Laws
Banned words: provenance, substrate, load-bearing, regime, support, honest(ly),
distill, ground (as verb), ruling. Comments max 2 consecutive comment lines in
new code (the commit hook enforces; if a commit is blocked, shrink comments, never
bypass). No em dashes anywhere.
