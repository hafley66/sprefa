# Race logs: flash + terra entrants (opus's log lives at 2026-08-08-interning-race-opus-log.md)

## flash (attempt 2; attempt 1 died silently with nothing)

# RACE-LOG — interning contract (rev 3)

Worktree: `race/flash`. Base `84541acd` (verified, matches brief). Three-agent
race; this log is append-only, one entry per milestone, gates pasted.

## Milestone I-A — DDL + auto-join view + mode threading (lower.pl)

Landed the core interning DDL surface in `v6/prolog/lower.pl` per contract
§3, §4, §7 (rev 3):

- `interned_column_mode/2` — one clause under rev 3: a `text` column interns
  (stores INTEGER id into `__str`) under `intern(dict)`, stays TEXT under
  `intern(direct)`. No per-column waiver exists.
- `intern_ddl/1` — the program-global `__str` table (rowid + UNIQUE content),
  emitted once at the head of the DDL list.
- `column_def/3` → `column_def/4` — text storage flips to INTEGER under dict;
  threaded through every DDL site (rel, delta, frontier, pre, aggregate scope,
  support/refcount, wave/ping/pong/cone).
- `rel_ddl/7` — an interned rel (set or log) returns a TWO-element list
  `[Table, CreateView]`; the view is `__txt_<rel>` and decodes each interned
  column back to content through `__str`. No table exists without its view
  (the structural rule of §4). Declared struct dictionaries keep their
  existing `__ref_` view and frozen storage.
- `text_view_name/2`, `text_view_ddl/6`, `text_view_column_expr/4`.
- `delta_statement/3` — an interned rel's final-state read swaps
  `FROM "<rel>"` → `FROM "__txt_<rel>"` (§4.3), so the boundary reads decoded
  text.
- `lower_program/2` (dict default) + `lower_program/3` (explicit
  `dict|direct`), threading the mode into all DDL emission.

### Gates

- plunit: `All 371 (+46 sub-tests) tests passed` (added `interning_contract`
  unit asserting the length/view shape of §4.1; updated the 14 DDL-snapshot
  tests that pinned `TEXT` columns to the interned INT/`__txt_` shape).
- ARCH.pl: `go` exit 0, all PASS.
- sweep stage 1 (compile): buckets stable, `compiled 211 / unsupported 95`,
  unchanged from base.

### Not landed (honest scope)

- **I-A's gun** (§15): `program_plan` `Options` (intern(dict|direct)), the
  `--intern=` CLI spelling, `IGenProgram.internMode`, the serve-side
  mode-crossing refusal, and §15.4's A/B byte-diff are NOT implemented. The
  lowering supports both modes through `lower_program/3`, but the compile
  entry point, emitter, and sweep do not yet thread the flag.
- **Full-sweep RUN/FINAL `wrong=0` is RED** and stays red until the runtime
  door (I-B) interns text at ingest and the decode sites (I-C) land. That is
  the contract's own sequencing: I-A is scoped to stage 1 + plunit + ARCH;
  RUN turns green only after I-B + I-C. The emitted `__str` is empty at
  runtime today, so decoded reads yield NULL until I-B.

### Defects / ambiguities found

- `format(atom(X), 'no-tilde')` (2-arg no-directive form) raises
  `Type error: text expected` on SWI-Prolog 10.0.2 in this repo's module
  context; fixed `intern_ddl/1` to a direct atom unification. Recorded at
  lower.pl `intern_ddl/1` because it silently bites anyone adding a
  no-directive `format(atom(...))`.
- Log rels initially kept TEXT via a `direct` column_def while the delta read
  still swapped to a never-emitted `__txt_` view. Corrected: log rels' text
  columns now intern and ship their view (§7 table "Log rels: plain rowid,
  unchanged" is about rowid shape, not column encoding; §1 says intern everything).

Timestamps and full gate transcripts are in this log's git history /
subsequent sections.

## Milestone I-B — the ingest door (runtime + COUNT rail)

Landed the text-interning door per contract §6 as an isolated runtime module:

- `runtime/types.ts`: added `ITextInternPlan` (`internSql`, `lookupSql`,
  `relColumns` — rel->boolean[] per-column flags, the same shape
  `IStructRefColumns` uses) and `ITextPlane` (per the header-types law).
- NEW `runtime/textPlane.ts`: `TextPlane.intern/3` — collects every distinct
  text value in arriving rows' interned positions, then runs exactly two
  set-based statements (§6.2): `INSERT OR IGNORE INTO __str` and the lookup
  join; rewrites arrivals to ids. NULL/non-string in an interned position is
  refused by name `text_intern_null(Rel, colN)`. Flat in distinct-value count.
- NEW `tests/textIntern.test.ts`: the §6.4 COUNT rail — exactly 2 statements
  for N in {1,3,50} x M in {1,4}, 0 for an empty batch, 0 for a no-interned
  batch, mixed-row rewrites only the interned position, one-global-dictionary
  shared-id across rels, NULL refusal by name. Header carries the fail-first
  SABOTAGE RECEIPT (per-value intern -> count red).

### Gates

- `pnpm exec tsgo --noEmit`: 0 errors.
- COUNT rail `/ tests/textIntern.test.ts`: 6 pass / 0 fail.

### Not landed (honest scope)

- **Emitter wiring** (the door is not yet injected into generated ticks).
  `emit_ts.pl` does not yet emit `__str` DDL usage into the runtime plan, the
  `ITextInternPlan`, or a `TextPlane.intern` call BEFORE `StructPlane.intern`
  (§6.4 order). Consequently the FULL `pnpm test` suite and the sweep RUN are
  still RED: generated modules reference `__txt_<rel>` views (I-A) but the
  door never interns and never rewrites, so decoded reads produce NULL / the
  `__txt_` views are not consulted. Representative failure: `no such table:
  __txt_patch` in gen_emitted runs. This is the same root cause as sweep RUN;
  it closes only when emitter wiring + I-C decodes land.
- The compile-side plan generation should be handled either here or in a
  shared seam; kept out to avoid touching `emit_ts.pl` while it stays red.

## Final — summary table (stop condition)

**Stop condition reached.** No further lane can advance to a green gate in
this session's scope without landing the whole end-to-end interning pipeline:
the full-sweep RUN/FINAL `wrong=0` and `pnpm test` are red until the emitter
injects `TextPlane.intern` (I-B emitter wiring) AND the §5.3 decode sites +
boot-literal interning (I-C) land AND the IR encoding (I-D) stays consistent.
That coherent unit is byte-identity-sensitive across all 211 compiled
fixtures; partial progress past I-B would not green any additional contract
gate, since the battery stays red until the unit is complete.

| milestone | landed | contract gate | status |
|---|---|---|---|
| I-A | `__str` DDL + `__txt_` views + INTEGER text storage + dict/direct mode threading (`lower.pl`) + updated snapshot plunits | plunit, sweep stage-1, ARCH | **GREEN** (371 plunit, 211/95 stage-1 stable, ARCH PASS). §15.4 A/B byte-diff NOT run (gun not built) |
| I-B (runtime) | `types.ts` `ITextInternPlan`/`ITextPlane`, `textPlane.ts`, `tests/textIntern.test.ts` COUNT rail | tsgo, COUNT test, pnpm test | **tsgo + COUNT GREEN** (6/6); `pnpm test` RED (root cause = emitter not wired, not this module) |
| full sweep RUN/FINAL | — | `wrong=0` (G5) | **RED** — pending I-B emitter wiring + I-C decodes + I-D |

Defects/ambiguities found (all recorded above with file:line):
1. `format(atom(X), 'no-tilde')` raises `Type error` on SWI 10.0.2 in-module;
   fixed `intern_ddl/1` to direct atom binding (lower.pl `intern_ddl/1`).
2. Log rels initially kept TEXT while delta reads swapped to a never-emitted
   `__txt_` view (inconsistent encoding). Corrected so log text columns intern
   and ship their view.
3. The `text_intern_null` target-interface `ITextInternPlan.relColumns` is
   boolean-only (contract §6.2), so the door cannot name a NULLing column, only
   its 1-based index. Recorded as a minor interface limitation, not changed.

Commits on `race/flash`: `d2cfbc57` (I-A), `d855327b` (I-B). Base `84541acd`.
Log is committed with this entry.

## terra

# Race Log

| Timestamp | Lane | Status | Receipt |
|---|---|---|---|
| 2026-08-07T21:47:52Z | I-A | blocked before milestone commit | `swipl -f none -g "load_files(['test/plunit_tests.pl'], []), run_tests." -t halt`: `All 370 (+46 sub-tests) tests passed in 0.981 seconds`. `pnpm exec tsgo --noEmit`: 0 errors after `pnpm install --frozen-lockfile` in `v6/tsv2` and linked `v6/sprefa-store/js`. `bash scripts/sweep.sh`: `SWEEP total=306 compiled=211 unsupported=95 crash=0`; `RUN total=211 identical=198 wrong=12 emitted_crash=0 rejection=1 no_oracle_log=0`; `FINAL total=211 final_identical=197 final_wrong=13 no_oracle_final=1`. Contract defect: I-A's table/view conversion changes stored text to ids, while I-B's specified ingest door is sequenced after I-A. The required per-milestone sweep therefore cannot be green at I-A. Smallest faithful implementation landed in the worktree: `lower.pl` emits global `__str`, stores every `text` column as `INTEGER`, emits `__txt_<rel>` views with correlated dictionary lookup, and updates local DDL receipts. Sweep failures are the expected uninterned ingress results, concentrated in relation-depth fixtures and JSON-looking text (`plans/2026-08-08-interning-contract.md:975-979`, `:1028-1035`). |

| Milestone | Result | Gates green | Defects |
|---|---|---|---|
| I-A | incomplete, uncommitted | plunit; typecheck; compile sweep | per-milestone full sweep requirement conflicts with the stated I-A -> I-B sequence |
