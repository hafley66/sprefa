# 6g — Smoke + benchmarks

Prove the path: cold walk once, warm walk never. Numbers go in the session
commit.

## Acceptance criteria

| scenario | current | target |
|---|---|---|
| g4 warm (sprefa self) | 134ms | 134ms (no regression) |
| swc cold run (first time) | 14s × rules | 1 walk + N index hits; <4s total |
| swc warm run (second time) | 14s × rules | all index hits; <200ms total |
| LSP reparse (1000 iter)    | n/a | no WT re-walk, stable RSS |

## Smoke script changes

```bash
# v2/tests/smoke/_3_large_repo_timed.sh
# Add: run twice; assert second run drops below threshold.

# Run 1 — cold
./_3_large_repo_timed.sh | tee cold.txt

# Run 2 — warm (no reset_state; state must persist)
./_3_large_repo_timed.sh --no-reset | tee warm.txt

# Expect:
#   cold.txt:  op fs ~3000 ms   # WT add + ls-tree + insert
#   warm.txt:  op fs <50 ms     # index hit
```

## Benchmark harness

```rust
# v2/benches/worktree_path_index.rs  (criterion)
fn bench_fs_cold_swc(c: &mut Criterion);
fn bench_fs_warm_swc(c: &mut Criterion);
fn bench_fs_fallback_libgit2(c: &mut Criterion);  # keep phase C visible
```

## Observability

```rust
# server stderr on SPREFA_TIMING=1
[timing]   path_index  hit=1067  miss=0      (0 ms)
[timing]   worktree    ensure=0  new=0       (0 ms)   # warm run
[timing]   worktree    ensure=1  new=1       (3000 ms) # cold run
```

## Disk budget check

After full swc bench run:
```
~/.cache/sprefa/wt/swc/<rev>/    ~60 MB  (WT, hardlinked objects)
store.sqlite blob_index rows     11k × (repo+rev+path+20B) ≈ 1 MB
```

Negligible at single-repo scale. Flag for review at 500-repo dogfood.

## Blast radius

- `v2/tests/smoke/_3_large_repo_timed.sh` — add double-run assertion
- `v2/benches/worktree_path_index.rs` — new, optional
- Session commit message carries the numbers

## Depends on / depended on by

- Depends: 6d, 6e
- Depended on: session close
