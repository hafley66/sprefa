# REPORT — a reference target with zero stored columns keeps its __id

Result: **STOP, no commits.** The coordinator's central diagnosis is refuted by
probe evidence. Implementing it as scoped touches files outside this lane's
ownership (analyze.pl, compile.pl, 0_type_plane.pl, registry.pl) and the
probe proves a lower.pl-only fix does not make probe C compile green.

## 1. The three probes, before / after

Compile driver: `bash v6/prolog/compile/scripts/compile_dl6.sh IN.dl6 OUT.ts`.

### Probe A — option(text) on a reference target. Compiles green before.
```
rel combo_pair(use_before: option(text), use_after: option(text)).
rel move_combo(id: int, normal: combo_pair).
```
```
wrote /tmp/probes/A.ts
CREATE TABLE "combo_pair" ("__id" INTEGER PRIMARY KEY, "use_before" INTEGER NOT NULL, "use_after" INTEGER NOT NULL, UNIQUE ("use_before", "use_after"))
```
Matches the coordinator. Parent keeps both columns; UNIQUE over both.

### Probe B — option(<rel>) with one ordinary column. Compiles green before.
```
rel combo_move(name: text, url: text).
rel combo_pair(label: text, use_before: option(combo_move)).
rel move_combo(id: int, normal: combo_pair).
```
```
wrote /tmp/probes/B.ts
CREATE TABLE "combo_pair" ("__id" INTEGER PRIMARY KEY, "label" INTEGER NOT NULL, UNIQUE ("label"))
CREATE TABLE "combo_pair__use_before" ("combo_pair_id" INTEGER NOT NULL, "combo_move_id" INTEGER NOT NULL, PRIMARY KEY ("combo_pair_id")) WITHOUT ROWID
```
Matches the coordinator. Column moves to a companion keyed on the parent's
`__id`; `__id INTEGER PRIMARY KEY` on the target.

### Probe C — every column option(<rel>). Stops today (fail-first receipt).
```
rel combo_move(name: text, url: text).
rel combo_pair(use_before: option(combo_move), use_after: option(combo_move)).
rel move_combo(id: int, normal: combo_pair).
```
```
{"code":"reference_target_has_no_columns/1","message":"rule-index unavailable: unsupported_construct: compiler refused rule 'reference_target_has_no_columns' for rel 'combo_pair/0' (reference_target_has_no_columns)",...}
unsupported_construct(reference_target_has_no_columns(combo_pair/0))
```

### What happens AFTER the coordinator's scoped change
I removed the stop (`0_generic_expand.pl:278-284`, returning the empty mirror)
and applied the coordinator's lower.pl fix (omit `UNIQUE (...)` when columns
is empty). Probe C does NOT go green. It fails at the emit/catalog phase:

```
cat: /tmp/probes/C_after.ts: No such file or directory
(compile_dl6 returns false — a silent goal failure, "no ball")
```

Root cause traced via targeted traces:
- `combo_pair` is **absent from AllRefs** after option expansion. It carries no
  `kind`/`col_type`/`keyed`/`keep` declaration — only an empty `type_decl`
  mirror, `option_column` markers, and `rel_module_decl`.
- `declared_refs/2` (`analyze.pl:253`) reads only `kind`, `keyed`, `keep`,
  `col_type`. It never sees `combo_pair`, so `combo_pair` is not in AllRefs → not
  in RefColumns → not in RefTypes → **not in RelPlans → no DDL at all**.
- `move_combo.normal = combo_pair` then points at a rel with no table; the
  catalog step `rel_row_id(combo_pair)` fails.
- Independent control: `rel zed().` (a zero-arity SET rel WITH `kind zed/0 set`,
  so it IS in AllRefs) also produces no table, because `rel_columns/6`
  (`analyze.pl:327-336`) calls `numlist(1, 0, Positions)` which fails, so
  `zed/0` never reaches RefColumns/RelPlans.

So the coordinator's premise — "the __id is already the identity (emitted
unconditionally on every reference target), so this is dropping a meaningless
constraint, not adding an identity case" — is refuted. A zero-column reference
target is dropped from the rel inventory entirely; there is no `__id` table to
keep. This is exactly the plan doc's recorded "GOAL FAILED, no ball"
(`plans/2026-08-11-option-list-rel-generic.md:227`) on the empty-mirror clause,
which the coordinator asked me to re-read and verify.

## 2. Two-parent distinctness fixture

Not built. Building it requires the compiler to first register a zero-column
reference target (`combo_pair`) as a rel and emit its `__id` table. Probe C
cannot compile green under the coordinator's scoped change, so there is no
emitted program to run a two-parent fixture against. A DDL-only receipt that
does not also prove two parents stay distinct would be the exact banned
"DDL-only receipt."

## 3. Sabotage receipt

Not run. There is nothing to sabotage: the coordinator's own change does not
reach the DDL for `combo_pair` (the rel is absent from RelPlans). Running the
two-parent fixture red-then-green is gated on probe C compiling, which it does
not under this scope.

## 4. The arrival path content-match site

The coordinator directed: find "the site where the arrival path stops looking
for a content match that cannot exist" and cite it.

Search of the arrival/dedup path shows the content-coupled site:
`v6/prolog/lower.pl:2822-2841` `set_arrival_sql_parts/4` — the unkeyed arm is
`INSERT OR IGNORE INTO` + `''` conflict (`lower.pl:2822`), and the keyed arm
builds `ON CONFLICT (<key cols>)` from `KeyPositions`
(`lower.pl:2823-2841`). For a zero-column target the key-column list is empty,
so both the `keyed` arm's `ON CONFLICT ()` and the ordering that implies a
content match are the degenerate sites. This is consistent with the plan doc's
agree with the coordinator's own plan (B-b: "every arrival mints a new row; no
dedup").

However this site is unreachable in a probe that never registers the rel, so it
is downstream of the registration gap.

## 5. Gate output (pristine tree, no committed change)

`cd v6/prolog && swipl -g go -t halt ARCH.pl`
```
PASS ... (roadmap_is_total, construct_status_closed, ... etc)  7 passes, EXIT 0
```

`cd v6/tsv2 && bash scripts/sweep.sh`
```
RUN total=284 identical=280 wrong=0 emitted_crash=0 rejection=4 no_oracle_log=0
FINAL total=284 final_identical=280 final_wrong=0 no_oracle_final=4
```
(4 rejections are pre-existing known-red JSON oneOf shapes, absent from
oracle too.)

`just green-all` (legs run individually):
- roundtrip: G1 389/390, one FAIL `mutual_recursion_matches_oracle` —
  **reproduces identically on main** (391/392 there). Pre-existing, not mine.
- text-door: `compiled=286 byte_identical=286 failures=0`
- plunit: 1 failing test `catalog_plane_rail:level_plane_family_corpus_counts`
  — **reproduces identically on main**. Pre-existing known-red (`CI-KNOWN-RED.md`).
- store: 75 pass, 0 fail
- dl: 98 pass, 1 skip, 0 fail
- conformance: 390 PASS, 0 FAIL

No gate regressed relative to the base.

## 6. Measured pokeapi G1

`v6/tsv2` roundtrip checker: **G1 = 4**, unchanged. All 4 are the zero-column
reference targets `move_detail__contest_combos__normal` / `__super`.

## 7. Verdict on the coordinator's diagnosis

Disagree. The coordinator's claim that this is "dropping a meaningless
constraint, not adding an identity case" is refuted by probe evidence: a
zero-column reference target is dropped from AllRefs/RelPlans entirely before
lower.pl runs, so there is no `__id` table to keep. This is the plan doc's own
recorded "GOAL FAILED, no ball" (`plans/2026-08-11-option-list-rel-generic.md:227`).
The change IS an identity/registration case (as the plan's B-b fork priced),
not a degenerated constraint.

## 8. What I did NOT do

- Did not delete the `0_generic_expand.pl` stop for good. Removing it without
  building registration turns a named stop into a silent wrong answer (the 
  banned anti-cheat case, and the exact failure this lane's own plan doc
  recorded). The tree is restored pristine.
- Did not edit lower.pl, analyze.pl, compile.pl, 0_type_plane.pl, or
  registry.pl. The real fix reaches those files, which belong to other lanes.
- Did not fabricate a DDL-only receipt or a two-parent fixture against a
  program that does not compile.
- Did not commit. Zero commits.
