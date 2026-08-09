# lane catalog456 steps 4-6

Three sequential commits (one per step), each gated, on branch
`lane/catalog-steps456` off `1b136cb0`.

| commit | sha | step |
| --- | --- | --- |
| 1 | `5310e406` | catalog: the level-statement plane rows (step 4) |
| 2 | `adcdf9e2` | catalog: the host/bind port rows (step 5) |
| 3 | `93c1afc9` | catalog: the storage child rows (step 6) |

## Per-step row counts per family

Measured over the conformance-fixture corpus the `catalog_plane_rail` walks (330
fixtures, `v6/prolog/conformance/fixtures`), derived by running the same
`catalog_all_rows` the seed renders:

| step | family | rows |
| --- | --- | --- |
| 4 | scope | 40 |
| 4 | refcount | 281 |
| 4 | refcount staging | 281 |
| 4 | expand | 8 |
| 4 | dred | 12 |
| 4 | avg accumulator | 2 |
| 4 | **subtotal** | **624** |
| 5 | port | 6 |
| 5 | port_response | 5 |
| 5 | **subtotal** | **11** |
| 6 | storage | 1694 |

### Step 4 delta vs the plan's 605

The plan's 605 was counted over the 220-module `compile/out` corpus
(plan sections 3, 7: refcount 273+273, scope 37, expand 8, dred 12, avg 2). The
rail's corpus is the 330 conformance fixtures, which is where I measure. The
measured 624 differs exactly at the common families:

| family | plan (220-modulus) | measured (330 fixtures) | delta |
| --- | --- | --- | --- |
| refcount + staging | 546 | 562 | +16 |
| scope | 37 | 40 | +3 |
| expand | 8 | 8 | 0 |
| dred | 12 | 12 | 0 |
| avg accumulator | 2 | 2 | 0 |

The rarest families (expand 8, dred 12, avg 2) match the plan's figures exactly,
which pins the same underlying DDL mint sites. The +16 refcount and +3 scope are
corpus-compositional: the conformance corpus carries a few more level heads and
aggregate heads than the compile corpus the plan counted. No structural break.

## Per-gate verbatim output

Gates are listed in the brief order; run after every commit.

### Commit 1 (step 4)

```
$ cd v6 && just plunit            # % [490/490] ... passed, exit 0
$ cd v6 && just conformance       # 330 PASS, exit 0
$ cd v6 && just text-door         # TEXT_DOOR compiled=231 byte_identical=231 failures=0
$ cd v6/tsv2 && bash scripts/sweep.sh  # SWEEP total=330 compiled=231 unsupported=99 crash=0
                                    RUN total=231 identical=230 wrong=0 emitted_crash=0 rejection=1 no_oracle_log=0
                                    MANIFEST_REASON_DIFF restated=0 args=0 bucket_moved=0 added=0 removed=0
$ git diff --stat v6/prolog/compile/out   # (nothing)
```

### Commit 2 (step 5)

```
$ cd v6 && just plunit            # % [492/492] ... passed, exit 0
$ cd v6 && just conformance       # 330 PASS, exit 0
$ cd v6 && just text-door         # TEXT_DOOR compiled=231 byte_identical=231 failures=0
$ cd v6/tsv2 && bash scripts/sweep.sh  # crash=0 wrong=0, manifest all zeros (as commit 1)
$ git diff --stat v6/prolog/compile/out   # (nothing)
```

### Commit 3 (step 6)

```
$ cd v6 && just plunit            # % [494/494] ... passed, exit 0
$ cd v6 && just conformance       # 330 PASS, exit 0
$ cd v6 && just text-door         # TEXT_DOOR compiled=231 byte_identical=231 failures=0
$ cd v6/tsv2 && bash scripts/sweep.sh  # crash=0 wrong=0, manifest all zeros (as commit 1)
$ git diff --stat v6/prolog/compile/out   # (nothing)
$ cd v6 && just green-all         # FAIL set == EXACTLY the expected 7, listed below
```

`green-all` expected-fail set (identical to the pre-change baseline):

```
FAIL  scale-floor      FAIL  memory-soak     FAIL  prolog-lint
FAIL  lsp-diags        FAIL  compile-speed   FAIL  typecheck
FAIL  rtkq-golden
GREEN ALL FAILED after 195s
```

`prolog-lint` fails on the repo's pre-existing `unused_export_candidate`
findings (e.g. `string_dictionary_table/1`, `print_dl_*`); nothing added in
steps 4-6 is flagged. `scale-floor` stmts/tick drifts past its gate; the rest are
pre-existing environment/typecheck/bench gates.

## Deviations

- **Step 4 row count** differs from the plan's 605 only by corpus: the rail and
  my measurement run the conformance fixtures (624), the plan counted the
  compile corpus (605). Exact family-level match on expand/dred/avg.
- **Environment setup, not a code change.** `just sweep` initially could not run:
  `rxjs` was absent from `v6/sprefa-store/js/node_modules` (the sandbox shipped
  without `pnpm install`). Installed with `pnpm install` (pnpm, not npm) in
  `v6/sprefa-store/js`; `package.json` already declared `rxjs ^7.8.2`, no tracked
  file changed. Sweep then passed and stayed passing through all three commits.
- **Pre-commit hook dependency**: the repo's pre-commit `comment-budget-rail.sh`
  required the `sprefa-extract` `extract` binary, which was not built. Built it
  with `cargo build --release --features cli --bin extract` (no tracked source
  change). All three commits went through the hook clean.
- **Step 5 thread re-open, scoped to the plane half.** PR #52 cut the `Decls`
  argument out of `catalog_rows/4` because it had no reader. Step 5 re-threads
  `Decls` into the PLANE half only (`catalog_row_ddl/10`,
  `catalog_all_rows/10`, `catalog_plane_rows/10`), where `sh_decl`/`bind_decl`
  mint `port`/`port_response` rows. `catalog_rows/4` (the decl half that feeds
  `emit_ts.pl:program_catalog_rows` and the byte-stable TS const) still takes
  no `Decls`. This is the first reader of the cut thread, and it does not leak
  the thread back across the split.
- **Step 4 ordering comment**: one line at the `lower_program` call site notes
  the ordering constraint (catalog_row_ddl must run after
  `level_statement_groups`).
- **Port arity reading.** Per ruling `effect_decl_no_arrow`: a `port` row's arity
  is the declared INPUT count, `type_id` is the demand rel for a `sh_decl` (or
  the bind rel itself for a `bind_decl`), and a `bind_decl` mints a port with no
  `port_response` child. `port_response` arity is the declared OUTPUT count.
- **Test additions**: step 4 corpus family-count rail (`level_plane_family_
  corpus_counts`); step 5 port-row case over `2_hosts_wiring.pl`'s lowerable
  fixtures; step 6 storage interned-vs-raw test. The catalog_g1 stability tests
  were updated for the new plane families (storage accounted in
  `plane_kind_for`; `catalog_all_rows_equals_decl_rows` plane count 9 -> 13).
