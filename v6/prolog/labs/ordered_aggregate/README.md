# Ordered aggregate lab receipts

The lab uses a memory-only `@libsql/client` connection. The SQL probe creates
the toy store, runs both order axes, runs `group_concat` with an inner
`ORDER BY`, nests `json(payload)`, and prints the two scoped recompute plans
for 10 and 1000 groups.

## Q1 and Q4

| axis | SQL spelling | Prolog draft | result |
| --- | --- | --- | --- |
| value sort | `json_group_array(item_name ORDER BY item_name)` | `msort(["pear","apple","orange"], SortedValues)` | `["apple","orange","pear"]` |
| explicit ordinal | `json_group_array(item_name ORDER BY ordinal)` | `keysort([1-"pear",3-"apple",2-"orange"], SortedPairs)` followed by `pairs_values/2` | `["pear","orange","apple"]` |

`oracle_ordering.pl` prints the Prolog result as canonical JSON text. The SQL
probe asserts the same two byte strings. The value-sort row is the V5 parity
shape from `json_group_array(arg ORDER BY arg)`. The ordinal row uses a second
column and preserves stream order after sorting by that column.

## Q2

The SQL probe runs:

```sql
group_concat(item_name, ' > ' ORDER BY ordinal)
```

SQLite 3.45.1 returns `pear > orange > apple` for the checked sample. The
aggregate accepts the separator and inner `ORDER BY`.

## Q3

The hand-drafted incremental statements are:

```sql
DELETE FROM aggregate_head
WHERE store_name IN (SELECT store_name FROM __agg_scope);

INSERT OR IGNORE INTO aggregate_head (store_name, items)
SELECT b0.store_name,
       json_group_array(b0.item_name ORDER BY b0.ordinal)
FROM star_row b0
WHERE b0.store_name IN (SELECT store_name FROM __agg_scope)
GROUP BY b0.store_name;
```

The plan output contains `SEARCH b0 USING INDEX star_row_by_store_name
(store_name=?)`, `USING INDEX ... FOR IN-OPERATOR`, and
`USE TEMP B-TREE FOR json_group_array(ORDER BY)`. It contains two statements
for both 10 and 1000 groups.

## Q5 and Q6

`nesting_probe.dl6` is accepted by the Prolog oracle at current HEAD. Its
oracle tick produces a nested array value. The requested `bop check` command
reaches the linked `sprefa-store` package and exits 1 because that package has
no resolvable `rxjs` installation in this worktree. The compiler result is
therefore recorded as an environment-blocked receipt, with no source change.

For a minus delta in one group, the scope seed names that group, the old head
row is deleted, and the grouped insert rebuilds the remaining rows in ordinal
order. When the last row is removed, `HAVING count(*) > 0` emits no replacement
head row. A later add to the same group creates a new head row.

## Commands

```text
node probe.mjs                         exit 0
swipl -q -s oracle_ordering.pl -g run -g halt   exit 0
cd v6/prolog/compile/scripts && swipl -q -l dl6_oracle.pl -g "oracle('../../labs/ordered_aggregate/nesting_probe.dl6','../../labs/ordered_aggregate/nesting_schedule.json')" -g halt   exit 0
cd v6/tsv2 && npm run --silent bop -- check ../prolog/labs/ordered_aggregate/nesting_probe.dl6   exit 1
```

## Coordinator re-verification (post-landing)

The exit-1 bop result above was an environment gap (the coordinator had
installed deps only in v6/tsv2, and the linked sprefa-store package had no
node_modules). After `pnpm install` in v6/sprefa-store/js the same command
prints `refusal: unsupported_construct(aggregate_head(json_array(_)))` and
exits 2 — the honest current-HEAD receipt: the compiler refuses json
aggregate heads by name, which is exactly what the wiring arc removes.

Observation for the json-potholes lane (do not fix here): the nesting oracle
tick renders the json payload cells as `#{a:2,z:1}` text, not canonical JSON —
the json-arrival mapping through dl6_oracle is the in-flight potholes
territory, and this output may legitimately change when that lane lands.
