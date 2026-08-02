# Lane A REPORT
## Base sha verified: yes + 92756b54dc0cb633e9636234f5358f3324be1ebf
`git merge --ff-only` returned "Already up to date."; `git rev-parse HEAD` printed `92756b54dc0cb633e9636234f5358f3324be1ebf`.

## Run 1 exit code: 0 (pipeline/tee exit); FAIL lines: 2, pasted below
The brief's command is `bash ... 2>&1 | tee ...` (no direct exit capture). I recorded `EXIT_CODE=0` from the tee pipeline. `receipts.sh` itself did not print an explicit failure/success trailer; it printed FAIL lines mid-run and "server died on boot". So the pipeline exit 0 reflects `tee`, not the harness success condition. run1.txt full capture is in lane-artifacts/run1.txt.

FAIL lines (each):
```
FAIL  phase 0: 6-ordinal.dl6 did not compile: ERROR: [Thread main] -g compile_dl6('/Users/chrishafley/projects/sprefa-lane-flash-a/v6/tsv2/labs/staged-writes/6-ordinal.dl6', '/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T//staged-writes.10OmA0/6-ordinal.ts'): rule-index unavailable: unsupported_construct: compiler refused rule 'host_column_shadows_runtime' (host_column_shadows_runtime)
FAIL  server died on boot:     at ModuleJob.syncLink (node:internal/modules/esm/module_job:162:33) {
  code: 'ERR_MODULE_NOT_FOUND'
```

Phases 0-5 compiled `ok`; phase 0 for 6-ordinal failed compile; the server never booted (ERR_MODULE_NOT_FOUND), so the run did not reach the crash/write segment.

## DB files found: (empty)
dbs.txt contains no entries. `find <WORKDIR> -name '*.db' -o -name '*.sqlite*'` over the newest workdir returned nothing. Therefore step 3's per-file loop iterated zero times; schema.txt and effect_cache_rows.txt are both 0 bytes.

## effect_cache schema: (none dumped — no db files exist in the newest workdir)
No `.db`/`.sqlite` files were present in the newest workdir (`/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T//staged-writes.zepMQg`), so no schema block was captured.

## effect_cache rows after run: (none dumped — no db files exist in the newest workdir)
No db files present, so no rows could be selected.

## Epsilon probe: serve_trail_grep + hosts_trail_grep verbatim

serve_trail_grep.txt (v6/tsv2/serve/main.ts, pattern `digest|trace|ordinal|wall|elapsed|span(`):
```
(empty — no matches)
```

hosts_trail_grep.txt (v6/dl/src/1_hosts.ts, pattern `digest|trace|ordinal|wall|elapsed`):
```
v6/dl/src/1_hosts.ts:3: * and HostRunner (the `?` probe machinery: demand rows -> digest-cached effect ->
v6/dl/src/1_hosts.ts:11: * reads deltas$ for __req_* inserts, digest-caches via effect_cache (the `?`
v6/dl/src/1_hosts.ts:15: * in-flight run per full digest (the cache row IS the lock); errors land as cache state
v6/dl/src/1_hosts.ts:19: * 0_types.ts types + 0_digest.ts (shared fold) + sprefa-store-engine (store
v6/dl/src/1_hosts.ts:27: * The digest fold uses 0_digest.ts's shared fold.
v6/dl/src/1_hosts.ts:35:import { foldRowDigest } from "./0_digest.ts";
v6/dl/src/1_hosts.ts:37:import { PerfTrace } from "./0_trace.ts";
v6/dl/src/1_hosts.ts:67:/** Same fold law as 2_schema.ts's rowDigest, shared via 0_digest.ts's foldRowDigest
v6/dl/src/1_hosts.ts:69: *  function computes BOTH halves of effect_cache's split digest, called with a
v6/dl/src/1_hosts.ts:75:  const digestRow: Row = { ...requestRow, __host: hostName };
v6/dl/src/1_hosts.ts:76:  return foldRowDigest(digestRow, ["__host", ...columns]);
v6/dl/src/1_hosts.ts:371:// HostRunner: reads deltas$ for __req_* inserts, digest-caches via effect_cache,
v6/dl/src/1_hosts.ts:471:    // Perf trace (0_trace.ts, seam 2): "cache_hit" and "error" below never reach a
v6/dl/src/1_hosts.ts:478:        sql: "SELECT full_digest FROM effect_cache WHERE full_digest = ?",
v6/dl/src/1_hosts.ts:490:          "INSERT INTO effect_cache(full_digest,identity_digest,host,state,requested_tick) " +
v6/dl/src/1_hosts.ts:491:          "VALUES (?,?,?,?,?) ON CONFLICT(full_digest) DO NOTHING",
v6/dl/src/1_hosts.ts:533:      // Perf trace: no further `await` between here and the publish, per 0_trace.ts's
v6/dl/src/1_hosts.ts:539:        sql: "UPDATE effect_cache SET state = ? WHERE full_digest = ?",
v6/dl/src/1_hosts.ts:544:      // escalation law): delete every OTHER full_digest sharing this identity_digest.
v6/dl/src/1_hosts.ts:551:        sql: "DELETE FROM effect_cache WHERE identity_digest = ? AND full_digest != ?",
v6/dl/src/1_hosts.ts:562:        .execute({ sql: "UPDATE effect_cache SET state = ? WHERE full_digest = ?", args: ["error", fullDigest] })
```

## Deviations: 
1. Run 1 produced 2 FAIL lines: (a) phase 0 `6-ordinal.dl6 did not compile`, compiler refused rule `host_column_shadows_runtime`; (b) `server died on boot` with `ERR_MODULE_NOT_FOUND`. The run died in compile/boot, before the crash/write segment, so no db was created by today's run.
2. Step 3 dumped nothing: the newest workdir (`staged-writes.zepMQg`) and the run's own work dir (`staged-writes.10OmA0`, named in the receipts.sh header) contain no `.db`/`.sqlite` files. dbs.txt, schema.txt, and effect_cache_rows.txt are all empty.
3. `serve_trail_grep.txt` is empty: pattern `digest|trace|ordinal|wall|elapsed|span(` matched nothing in `v6/tsv2/serve/main.ts`.
4. Beyond the listed commands I ran one extra `ls -la` over the leftover `staged-writes.*` dirs (to confirm where the run's artifacts went). Observation: the two other Aug 2 dirs (`.10OmA0`, `.TZjGov`) likewise contain no db; the Jul 30 dirs contain `crash.sqlite`, but per Step 3 only the newest dir was to be examined, so those older `crash.sqlite` files were not dumped.

Note (observation, not from a listed command): hosts_trail_grep shows today's `effect_cache` schema is `(full_digest, identity_digest, host, state, requested_tick)` — a demand/identity content-addressed split with no disk-digest column (full_digest is spent as the in-flight lock and identity key). Whether that constitutes the "already partially exists" trail is left to the coordinator.
