# A1 lower+check keep findings

| program | lower before | lower after | check before | check after |
|---|---|---|---|---|
| hello (`0_hello.dl7`) | 8 runs | 6 runs + 2 hits | 6 runs | 2 full + 1 delta + 3 hits |
| registry (`tsi_registry_emit.dl7`) | 18 runs | 18 runs + 1 hit + 1 key-differs | 6 runs | 2 full + 1 delta + 3 hits |

Differing key components: registry prelude lower key differs on `imports` (reported, run re-executed); hello none.
Fallbacks: none fired (`check_fallback` absent in both runs); delta guard `prelude_edge` and prefix guards never triggered.
Byte identity: stdout exact for both; stderr identical modulo timestamps and `install_ns` counters.
Commits: `17d7b3811` counters + `--trace`; second commit this lane.
