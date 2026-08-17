# soopy-extract-host-reads

Issue: `issues/soopy-extract-host-reads/item.md` (epic soopy-full-wiring).
Measured base: `plans/2026-08-16-soopy-extract-entanglement.md` sections 4.2,
4.4, collapse candidate 1.

## First action

```bash
git merge --ff-only f7ed05fa434aab8808c5a833a2bd94cb8448aead
```

Failure = STOP AND REPORT.

## Ownership

You own ONLY `v6/sprefa-engine-rs/src/hosts.rs` and
`v6/tsv2/goldens/scip_combo/**`. Forbidden: `v6/sprefa-extract/**`
(a sibling lane owns `project.rs`), everything else.

## The defect

`SprefaExtractExecutor` (`hosts.rs:807-892`) reads bytes with
`std::fs::read(&path)` at `hosts.rs:852` — always current worktree disk. The
plan carries a `digest` input (a git blob oid from `repo_files_at`,
`gen_served/44a6494405222cdc9132718fe4d7e7ae.dl6:116-117` and its comment at
:113-115): digest is in the memo key (`hosts.rs:1407-1425` ordered_inputs) but
never gates content. Cache hit or miss, facts come from current disk.

## The fix, pinned decisions (do not re-decide)

1. **Bytes come by blob oid.** When the plan's inputs include a `digest`
   value, resolve the repo root (the `repo` input when present, else
   `soopy::discover` from the path) and read the blob with
   `soopy::GitBatch::open(root)` + `.read(&ObjectId(digest))` — the exact
   batched shape at `src/change_facts.rs:193-205`. No rev string, no
   read-then-verify.
2. **No digest = worktree read stays.** A demand without a `digest` input
   keeps the current `std::fs::read` path. Name this branch in one comment.
3. **Keep a long-lived GitBatch per repo** in the executor (Mutex<BTreeMap>
   keyed by repo root, same pattern as the other executors' memos). Never one
   spawn per blob (N+1 law).

The input VALUES are available where the applicative group key is built
(`hosts.rs:1407-1425`); plumb the digest input into the executor call rather
than re-parsing the template. Read how `ordered_inputs` flows before coding.

## The gate inversion (trap)

`v6/tsv2/goldens/scip_combo/2_extract_rev_skew.dl6:1-27` asserts the pinned
reads DISAGREE (it documents this bug). After the fix it must assert
AGREEMENT: update the fixture + `just scip-combo` gate so green means "digest
gates content". State the inversion in the fixture header. `ARCH.pl:879`
defect D1 is related; do not edit ARCH.pl (coordinator owns it).

## Receipts

- `cargo test -p sprefa-engine-rs` rc=0, run twice, counts in the PR body.
- `just scip-combo` (from v6/tsv2) green, output pasted.
- FAIL-PRE-FIX: sabotage the digest branch (feed a stale oid) and show the
  old path returned worktree bytes; restore.
- `grep -n 'std::fs::read' v6/sprefa-engine-rs/src/hosts.rs` output in the PR
  body: only the no-digest branch remains.

## Style laws

tracing only, no eprintln. Banned words: provenance, substrate, load-bearing,
regime; "refusal" banned in prose. Comment budget: constraints only.
Descriptive names. Commit trailer: `Refs-Issue: @soopy-extract-host-reads`.

## Landing

Branch, commit, push, `gh pr create` with receipts. Do not merge. Lanes never
spawn subagents.
