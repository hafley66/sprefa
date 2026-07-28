# DIFF REVIEW FINDINGS (opus, main...cleanup/2026-07-27-reconcile, 42 commits)

Reviewer ran read-only, reproduced findings with probe scripts (scratchpad,
not retained), baselines all green before hunting. Dispositions below carry
the user's 2026-07-27 late rulings.

## Findings + dispositions

1. **Reload re-fires an in-flight effect** (1_hosts.ts:411, CONFIRMED,
   reproduced: ledger showed the shell command twice, one done row).
   `replayableRequests` deletes ALL pending rows at subscribe; a program
   reload while an effect runs loses the old lock and re-fires.
   **USER RULING: accepted behavior, no fix** ("no, call it again after dead,
   its fine") — consistent with effect_abort = best-effort: cancellation and
   dedupe across a swap are world-cost optimization, never semantics. Future
   reviewers: do not re-flag; the endurance pending-witness wedge (phase 1)
   is the place this gets revisited if cost receipts demand it.
2. **One stalled program POST blocks all loads up to 300s** (6_http.ts:549,
   CONFIRMED, reproduced with a held socket; unrelated GETs unaffected).
   readBody sits under the load-serializing concatMap. **FIX DISPATCHED**:
   per-request body read/parse/400 concurrently; only accepted programs enter
   the serialized swap.
3. **SSE client spanning a reload orphaned, socket never ends**
   (6_http.ts:377, CONFIRMED socket-level; pre-existing, docblock now
   overclaims). Also blocks DlServer.close(). **FIX DISPATCHED**: end the
   response when the inner completes; docblock made true.
4. **Stale cx_row/rel_tag/delta after program swap on same db**
   (engine.ts:203/942 + 3_runtime.ts:793/821, CONFIRMED latent). Positional
   tags + ON CONFLICT DO NOTHING leave the previous program's tag meanings.
   Zero readers today (rel_tag/delta write-only; cx_row only via
   retractThroughSupport tests). **BANKED**: trap for the first reader;
   revisit when a reader lands or at the P0 tick-log arc if it reads delta.
5. **IF NOT EXISTS hides schema drift** (engine.ts:203, PLAUSIBLE); scratch
   tables at engine.ts:726/731/1149 + lowerSql.ts:156 still bare CREATE.
   **BANKED** with 4 (schema-version check is the shape).
6. **Subscribe ratchet scans dl/src only** (one-subscribe.sh:7). Store src
   currently clean (6 lab-harness sites excluded), so count honest, rail
   narrow. **QUEUED small**: widen glob + drop the stale "lower BASELINE"
   advice line.
7. **goal-endurance can grade a squatter server** (hardcoded port 7191, any
   listener passes readiness; three orphaned main.ts servers found bound on
   7192/7373/17272, pids 11983/56400/58157). **BANKED**; orphan kill is a
   user paste (see below).
8. Nit batch (BANKED, one sweep later): endurance dead readiness line :36,
   double stop_server on fail :29, $WORK autopsy dirs unrotated (12
   accumulated), unguarded cd :18 (SC2164); upsert_edges builds VALUES
   before empty guard; toArray() retains unread QueryResults in runAll/
   executeAll$/exec/ingest returns; 6_http 404-fallthrough + SSE writeHead
   outside catchError; partition = two request listeners; measure.ts:23/57
   banned-word prose rename.

## Checked and clean (do not re-litigate without new evidence)

commit()-throw vs swap race (microtask window unreachable via router);
narrow unknown-rel catch (only 3_runtime.ts:958 throws there); 200-after-
subscribe ordering; boot-replay vs live-delta double-fire (serialized by one
concatMap); bigint flow; EMPTY-in-concat sites; N+1; stmt counters; banned
identifiers; header contracts; prolog corpus integrity; lsp-v5-bridge.sh.

## User-side paste (orphan dev servers)

kill 11983 56400 58157   # stale main.ts servers on 7192/7373/17272
