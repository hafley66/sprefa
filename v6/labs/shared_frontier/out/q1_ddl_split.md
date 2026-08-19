### Q1d. pokeapi DDL split, and the arm-B projection

| group | statements | bytes | share |
| --- | --- | --- | --- |
| durable (typed tables, dictionaries, catalog, their indexes and views) | 2160 | 715,709 | 42.5% |
| per-relation transient (__delta_, __frontier_, __next_frontier_, __support_next_, __new_ and their indexes) | 4688 | 966,907 | 57.5% |
| total emitted DDL | 6848 | 1,682,616 | 100.0% |
| arm-B shared replacement | 2 | 416 | 0.0247% |
| arm-B projected DDL total | 2162 | 716,125 | 42.6% |
