# Lane `chore-extract-cleanup-bench` (glm53f): cleanup pass 2, tests + bench plumbing

User 2026-08-31: second cleanup pass. NO behavior change, provable.
A live lane owns src/lang/** — FORBIDDEN here. Also FORBIDDEN: src/** all
of it, RATCHET.tsv, plans/** except none.

## Ownership
v6/sprefa-extract/tests/** only.

## First action
```
git merge --ff-only <BASE_SHA>
cd v6/sprefa-extract && cargo test --release --features cli 2>&1 | tail -3  # green BEFORE
```

## Work, priority order (commit per item, ~5 commits max)
1. tests/bench/mod.rs: GoProjection/RustProjection duplication — extract the
   shared shape into one helper where they genuinely share semantics; do
   NOT unify behavior that differs (read both carefully; a difference is a
   fact, keep it).
2. Dead test helpers: fns/consts in tests/** nothing references; delete.
3. Comment budget sweep over tests/**: keep TEST-header sabotage/fail-first
   receipts (repo law says they STAY), delete change-log narrative and
   restating comments.
4. Numbered-test hygiene: any test file with an `#[ignore]` dump that a
   later arc obsoleted (check 79_rust_type_dump against the merged type
   arcs) gets a one-line header saying what it is for, or gets deleted if
   its output dir no longer exists.
5. Naming in touched code only: single-letter locals get descriptive names.

## Proof
`cargo test --release --features cli` green before AND after; goldens
byte-identical; `just extract-ratchet` once at the end nice -n 15 — but
COORDINATE: if another lane is in its final gate, wait; one ratchet on the
machine at a time.

PR to MAIN, per-commit list + line-count delta. Done/blocked:
`boop beep --no-wait --as chore-extract-cleanup-bench sprefa-coordinator "<one line>"`.
