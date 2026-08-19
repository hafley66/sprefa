### Q2d. Shared-frontier key order, 256 relations x 200 retained ticks, median of 5

| frontier PRIMARY KEY | frontier rows | reads per run | median ms | us per read |
| --- | --- | --- | --- | --- |
| B  (relation_id, row_id, tick, sign) | 51200 | 256 | 6.88 | 26.9 |
| B' (relation_id, tick, row_id, sign) | 51200 | 256 | 5.36 | 20.9 |

#### B  (relation_id, row_id, tick, sign)

```
SEARCH f USING COVERING INDEX sqlite_autoindex_frontier_1 (relation_id=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

#### B' (relation_id, tick, row_id, sign)

```
SEARCH f USING COVERING INDEX sqlite_autoindex_frontier_1 (relation_id=? AND tick=?)
SEARCH typed USING INTEGER PRIMARY KEY (rowid=?)
```

