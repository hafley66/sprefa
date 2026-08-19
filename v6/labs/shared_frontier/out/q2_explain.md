### Q2c. EXPLAIN QUERY PLAN, N=64, 4000 durable rows per relation, 1 frontier row per relation

#### arm A, no ANALYZE

```sql
SELECT typed."__id", typed."row_key", typed."value_a", typed."value_b" FROM "__frontier_rel_7" f JOIN "rel_7" typed ON typed."__id" = f."row_id" WHERE f."_phase" = ?
```

```
SEARCH f USING INDEX __frontier_rel_7_phase (_phase=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

#### arm A, after ANALYZE

```sql
SELECT typed."__id", typed."row_key", typed."value_a", typed."value_b" FROM "__frontier_rel_7" f JOIN "rel_7" typed ON typed."__id" = f."row_id" WHERE f."_phase" = ?
```

```
SEARCH f USING INDEX __frontier_rel_7_phase (_phase=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

#### arm B, no ANALYZE

```sql
SELECT typed."__id", typed."row_key", typed."value_a", typed."value_b" FROM "frontier" f JOIN "rel_7" typed ON typed."__id" = f."row_id" WHERE f."relation_id" = ? AND f."tick" = ?
```

```
SEARCH f USING COVERING INDEX sqlite_autoindex_frontier_1 (relation_id=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

#### arm B, after ANALYZE

```sql
SELECT typed."__id", typed."row_key", typed."value_a", typed."value_b" FROM "frontier" f JOIN "rel_7" typed ON typed."__id" = f."row_id" WHERE f."relation_id" = ? AND f."tick" = ?
```

```
SEARCH f USING COVERING INDEX sqlite_autoindex_frontier_1 (relation_id=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

