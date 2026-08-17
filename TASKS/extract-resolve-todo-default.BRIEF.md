# Lane brief: delete the `Resolve<F>` todo!() default body (issue extract-resolve-todo-default)

First action: `git merge --ff-only 725d06804c63f056973a3853a066eeb84db6d82e`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

Do not ask a question before starting. Everything you need is below with a
`path:line`. If a receipt does not match what you read, say which one and stop.

## The decision, already made by Chris. You implement it, you do not re-open it.

`issues/extract-resolve-todo-default/item.md`, Decisions section:

> Fork B: delete the todo!() default body; Resolve becomes non-defaulted and the
> compiler demands an arm (or an explicit empty impl) per Source.

Fork A (default body returning `Vec::new()`) is REJECTED. Do not write a
blanket impl, do not write a `Vec::new()` fallback anywhere, do not add empty
`impl Resolve<TypeF>` blocks for languages that have no type plane. A language
with no arm must have NO impl block: absence is the correct, compiler-visible
state, and the `RESOLVE_ARMS` rows with `None` already encode it at dispatch.

## The defect, with receipts

| fact | receipt |
|---|---|
| the default body | `v6/sprefa-extract/src/types.rs:1126-1129`, `fn resolve(...) { let _ = (output, cx); todo!("4b-iii landed ...") }` |
| the freeze that put it there, now expired | `v6/sprefa-extract/src/types.rs:843-846` ("The trait surface + types only. Every method body is todo!(); NOTHING calls resolve; no impl exists yet") and the module header `types.rs:8-12` |
| eight live impls, all with real bodies | `lang/ts.rs:2602`, `ts.rs:2729`, `lang/rust.rs:774`, `rust.rs:907`, `lang/go.rs:1544`, `go.rs:1681`, `lang/kotlin.rs:1145`, `lang/prolog/_0_source.rs:795`, `lang/dl6/_0_source.rs:425`, `dl6/_0_source.rs:449` |
| the hand-maintained dispatch guard that stands in for the compiler today | `v6/sprefa-extract/src/project.rs:446-497` (`RESOLVE_ARMS`) |
| that guard already failed once | issue @extract-dl6-resolve-unwired: `DlSource` had both arms and neither was dispatched |

Because every live impl already writes a body, deleting the default is expected
to be a GREEN no-op at compile time. That is the point: the change removes a
reachable `todo!()` panic from a shipped binary and converts a future missing
arm from a runtime panic into a compile error.

## Exact fix

### 1. `v6/sprefa-extract/src/types.rs:1122-1130`

Make `resolve` a required method: keep the signature and its doc comment, delete
the body and the two lines inside it, leave `;`.

```rust
pub trait Resolve<F: Family>: Source {
    /// <keep the existing doc comment verbatim>
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<F>>;
}
```

Add ONE constraint line to the trait doc, in the file's voice, no history:
a `Source` with no plane for `F` implements nothing for `F`; there is no empty
default, so a missing arm is a compile error, never an empty result.

### 2. Stale prose in files you own

Three comments in `types.rs` state the expired freeze premise. Correct each to
what the code now is; do not narrate the change, do not date it, do not name an
arc or a commit:

- `types.rs:8-12` ("bodies are todo!(), and NOTHING calls resolve yet")
- `types.rs:843-846` ("Every method body is todo!(); NOTHING calls resolve; no
  impl exists yet ... Human review gates 4b")
- `types.rs:1875` ("seed Extract trait is todo!()") ; this one is about the SEED
  crate's trait, NOT `Resolve`. Read it, and leave it alone if it is accurate.

### 3. Do NOT touch `project.rs`

`project.rs:47-48` and `project.rs:446-447` both justify themselves with "the
trait's default body is `todo!()`". Those two comments go stale with your
change. `project.rs` is owned by another lane and is FORBIDDEN to you. Name
both `path:line` in your PR body under a `Follow-up` heading, with the exact
replacement wording you would have written. Do not edit them.

## Receipts your PR body must carry

1. **The compile-enforcement demonstration, done once and reverted.** Delete the
   `fn resolve` body+signature from `impl Resolve<CallF> for PrologSource`
   (`lang/prolog/_0_source.rs:795`), run `cargo build --features cli`, paste the
   `error[E0046]: not all trait items implemented, missing `resolve`` output
   verbatim, then `git checkout -- src/lang/prolog/_0_source.rs` and confirm
   `git status` is clean of it. This demonstration is NEVER committed.
2. **`rg -n 'todo!' v6/sprefa-extract/src/` before and after.** Before: 6 hits.
   After: the `Resolve` default hit at `types.rs:1128` is gone; report the rest.
3. Both full test runs with their counts (below).

## Gate, run each twice, echo rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
```

Baseline at the base sha is rc=0 with every leg green. Paste the `test result:`
summary line of EVERY test binary from both runs; the two runs must agree
binary-for-binary. A leg that differs between runs is a report-and-stop.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/types.rs`

If the build demands a second file, STOP and report which one and why, with the
compiler error. Do not open it.

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/project.rs`
- every existing file under `v6/sprefa-extract/tests/` (new test files only, and
  this card needs none)
- `v6/sprefa-engine-rs/src/hosts.rs`, `v6/tsv2/goldens/scip_combo/**`
- every `.v5.jsonl` baseline (they are the oracle; never regenerate one)
- everything outside `v6/sprefa-extract/`

Concurrent lanes own the forbidden files.

## Laws that bind you

- Never spawn a subagent.
- Every public type lives in `types.rs`, the canonical type module.
- Comment budget: comments state only constraints the code cannot show. No
  change-log narrative, no dates, no arc or commit references, no restating the
  next line.
- dl/rust identifiers are descriptive, never single-letter.
- No em dashes. Banned in prose AND identifiers: provenance, substrate,
  load-bearing, regime. "refusal" is banned in prose; say TODO or not built yet.
- No `eprintln!` under `src/**`; `tracing` only.
- Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.

## Landing

1. You are already on branch `fix/extract-resolve-todo-default` in your worktree.
2. Commit. The commit message body carries the receipts above.
3. `git push -u origin fix/extract-resolve-todo-default`
4. `gh pr create` with a body carrying: the defect + its `path:line`, the
   decision citation, the E0046 demonstration output, the `rg todo!` before and
   after, both gate runs with per-binary counts, and the `Follow-up` heading
   naming the two stale `project.rs` comments.
5. The PR body ends with the trailer line: `Refs-Issue: extract-resolve-todo-default`
6. NEVER merge the PR. Report the URL and stop.
