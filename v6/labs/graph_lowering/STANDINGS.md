# graph_lowering STANDINGS

measured 2026-08-20T16:23:05.777Z, libsql :memory:, node v24.15.0

| program | fixture | nodes | edges | materialised rows | statements | ms |
|---|---|---|---|---|---|---|
| reach | chain-200 | 200 | 199 | 200 | 1207 | 39 |
| tiers | chain-200 | 200 | 199 | 200 | 1209 | 40 |
| components | chain-200 | 200 | 199 | 40000 | 1209 | 353 |
| distance | chain-200 | 200 | 199 | 200 | 1209 | 37 |
| triangles | chain-200 | 200 | 199 | 0 | 4 | 0 |
| reach | chain-1000 | 1000 | 999 | 1000 | 6007 | 276 |
| tiers | chain-1000 | 1000 | 999 | 1000 | 6009 | 259 |
| components | chain-1000 | 1000 | 999 | 1000000 | 6009 | 44047 |
| distance | chain-1000 | 1000 | 999 | 1000 | 6009 | 277 |
| triangles | chain-1000 | 1000 | 999 | 0 | 4 | 1 |
| reach | grid-16x16 | 256 | 480 | 256 | 193 | 6 |
| tiers | grid-16x16 | 256 | 480 | 256 | 195 | 6 |
| components | grid-16x16 | 256 | 480 | 65536 | 195 | 182 |
| distance | grid-16x16 | 256 | 480 | 256 | 195 | 7 |
| triangles | grid-16x16 | 256 | 480 | 0 | 4 | 1 |
| reach | grid-32x32 | 1024 | 1984 | 1024 | 385 | 18 |
| tiers | grid-32x32 | 1024 | 1984 | 1024 | 387 | 14 |
| components | grid-32x32 | 1024 | 1984 | 1048576 | 387 | 5001 |
| distance | grid-32x32 | 1024 | 1984 | 1024 | 387 | 15 |
| triangles | grid-32x32 | 1024 | 1984 | 0 | 4 | 3 |
| reach | two-grids-16x16-plus-triangle | 515 | 963 | 256 | 193 | 7 |
| tiers | two-grids-16x16-plus-triangle | 515 | 963 | 256 | 195 | 6 |
| components | two-grids-16x16-plus-triangle | 515 | 963 | 131081 | 195 | 372 |
| distance | two-grids-16x16-plus-triangle | 515 | 963 | 256 | 195 | 7 |
| triangles | two-grids-16x16-plus-triangle | 515 | 963 | 6 | 4 | 1 |

`materialised rows` is the set the post-recursion aggregate reads: level/hop carry every depth a node is seen at, label every candidate id. A monotone aggregate inside the recursion would keep one row per key.
