# V8 flash work review

Base: `baf09ef954021d328667902fda1573594e7bd8f7 Merge pull request #761 from hafley66/docs/v8-wave3-briefs-20260914`

## Contents

- [Round 1: hafley-rs PR 65](#round-1-hafley-rs-pr-65)
- [Unexamined](#unexamined)

## Round 1: hafley-rs PR 65

Verdict: **rework**

Reason: the committed validation command fails, the root `-p` command cannot resolve the excluded package, and the regenerated wire golden contains changes outside the named path substitution.

Scope: `hafley66/hafley-rs` PR `#65`, base `3ee65276d058ccc1beade6888dc2789dfd605d28`, head `54db5a93cf59f382d7898e217da1dbf30e3c0cbd`.

### Claim versus evidence

| Claim | Command | Observed | Match |
|---|---|---|---|
| PR diff is `591` additions and `583` deletions | `git diff --numstat 3ee65276... 54db5a93... \| awk ...` | `files=20 additions=591 deletions=583` | yes |
| `8` `.v5.jsonl` baselines contain only `v6/sprefa-extract` to `crates/sprefa-extract` replacement | For each changed baseline: `git show BASE:file`, `sed 's#v6/sprefa-extract#crates/sprefa-extract#g'`, then `cmp` against `git show HEAD:file` | `37` removed and `37` added lines across the `8` files; transformed parent files compare byte-identical to head | yes |
| `kind_vocab/wire_golden.jsonl` was regenerated through the binary | `git diff --numstat BASE HEAD -- .../wire_golden.jsonl`; compare head against the parent after the named substitution | `534` removed and `534` added lines; the parent has `0` old-path hits; residual diff is `534/534` | regeneration occurred; correctness remains open |
| `3` binary `.scip` fixtures stay byte-identical | `git diff --name-only BASE HEAD --` followed by the three fixture paths | `binary_scip_changed=0` | yes |
| `5` tests gained `#[ignore = "needs rust-analyzer on PATH"]` | `git diff --unified=0 BASE HEAD --` the three test files | `5` added attributes with the exact reason | yes |
| The parent-state failures of those `5` tests are caused by missing `rust-analyzer` | `cargo build --locked --features cli --bin extract`; then one exact `cargo test --locked --features cli --test ... -- --exact` per test | all `5` exit `101`; each output names `Unknown binary 'rust-analyzer' in official toolchain` | yes, after building the required CLI binary |
| `rust-analyzer` is absent from PATH | `command -v rust-analyzer; rustup which rust-analyzer` | PATH contains the rustup shim at `/Users/chrishafley/.cargo/bin/rust-analyzer`; rustup reports `unknown binary` | wording mismatch; executable unavailable |
| `cargo test -p sprefa-extract` gives `918 passed, 0 failed, 21 ignored` | From PR head crate directory: `cargo test --locked -p sprefa-extract` | exits `101`; `104_tier_decline_diagnostic` has `0 passed`, `4 failed`, caused by missing `extract` binary | no |
| `16` ignores pre-existed and `5` were added | `git grep -h '#\[ignore' REV -- crates/sprefa-extract/tests \| wc -l` at base and head | base `20`; head `25` | no |
| `cargo test -p sprefa-extract` works from the repository root | From hafley-rs root at head: `cargo test --locked -p sprefa-extract` | exits `101`: package ID does not match any package | no |
| Empty `[workspace]` makes the crate build standalone | From PR head crate directory: `cargo test --locked`; repeated with `-p sprefa-extract` | manifest resolves standalone; both runs reach tests, then fail the same `4` real-binary tests | partial |
| Parent workspace still contains `sprefa-extract` | `git show HEAD:Cargo.toml \| rg -n 'workspace|members|exclude|sprefa'` | root glob is `crates/*`, but `crates/sprefa-extract` is explicitly excluded | no |
| `2` timing tests failed once and passed on rerun | No isolated reproduction command was present in the PR; current exact suite stops at an earlier test target | unverified | no |
| Clippy reports `162` warnings in `src/` and `1` in `tree-sitter-dl6/build.rs` | Not run | unverified | no |

### Golden and oracle classification

| File set | Mechanical removed | Mechanical added | Other removed | Other added | Method |
|---|---:|---:|---:|---:|---|
| `go/docs`, `kotlin/sample`, `python/docs`, `python/flow`, `rust/docs`, `rust/sample`, `ts/docs`, `ts/lambdas` `.v5.jsonl` files | 37 | 37 | 0 | 0 | Apply the named substitution to the complete parent file and compare byte-for-byte with head |
| `kind_vocab/wire_golden.jsonl` | 0 | 0 | 534 | 534 | Parent contains no named old path; transformed parent still differs by every changed line |

Representative other change at `crates/sprefa-extract/tests/fixtures/kind_vocab/wire_golden.jsonl:6129`:

```json
{"record":"data_doc","family":"data","ordinal":41,"span":{"start":843,"end":907},"format":"jsonl","doc":null}
```

The parent row at that position ends at byte `848`. This is a span and record-boundary change, not a path-prefix replacement.

### Findings

#### P0. The wire golden changes outside the stated path repair

- What: `534` removed and `534` added wire rows are classified as other. The file contains `0` occurrences of the old path at the parent revision.
- File: `crates/sprefa-extract/tests/fixtures/kind_vocab/wire_golden.jsonl:6129`
- Why it matters: regeneration from the binary under test does not establish that changed spans, ordinals, and row boundaries are correct. The oracle is open under the review rubric.
- One-line fix: restore the wire golden, then update only rows justified by an independent expected-output source.

#### P1. The reported test receipt does not reproduce

- What: `cargo test --locked -p sprefa-extract` from the standalone crate exits `101` with `4` failures because `tests/104_tier_decline_diagnostic.rs:40` cannot find the `extract` binary.
- File: `crates/sprefa-extract/tests/104_tier_decline_diagnostic.rs:40`
- Why it matters: the PR states `918 passed, 0 failed, 21 ignored`, but its stated command is red in the reviewed head.
- One-line fix: run the suite with the CLI feature and record the exact resulting receipt, or make the stated default-feature command provide its required binary.

#### P1. Root package validation and standalone workspace configuration cannot both satisfy the PR claim

- What: the root manifest explicitly excludes `crates/sprefa-extract`; the added empty `[workspace]` makes that directory its own workspace; root `cargo test --locked -p sprefa-extract` exits `101` because the package is not present.
- File: `crates/sprefa-extract/Cargo.toml:250`
- Why it matters: the commit message claims a `-p` run without naming the crate directory, while the requested root command cannot resolve. Only standalone manifest resolution is real, and its test run remains red.
- One-line fix: state and validate the supported invocation from the crate directory, or change the parent workspace contract in separately authorized scope.

#### P1. Manifest and lockfile edits exceed the brief ownership set

- What: the PR modifies `crates/sprefa-extract/Cargo.toml` and `crates/sprefa-extract/Cargo.lock`; ownership names only `tests/**`, `schema/*.md`, and `reports/*.md`.
- File: `crates/sprefa-extract/Cargo.toml:250`; `crates/sprefa-extract/Cargo.lock:3582`
- Why it matters: these two files are outside the lane’s declared write set and change Cargo workspace and dependency resolution behavior.
- One-line fix: remove the two files from this PR and handle standalone workspace configuration under an amended brief.

#### P2. The pre-existing ignore count is misstated

- What: source counts are `20` ignore attributes at the parent and `25` at the head; the PR says `16` pre-existing and `21` total.
- File: PR body validation section
- Why it matters: the ignored-test receipt cannot be reconciled with the checked source counts.
- One-line fix: report the observed test-harness ignore totals and separately list ignored source attributes if both measures are useful.

#### P2. Path parity checks call library functions directly

- What: `tests/19_docs_lang_arms.rs` imports and calls `dispatch` and `flatten`; `golden_parity.rs::v6_ported` also performs in-process extraction.
- File: `crates/sprefa-extract/tests/19_docs_lang_arms.rs:6`; `crates/sprefa-extract/tests/golden_parity.rs:175`
- Why it matters: these checks do not establish process invocation, CLI path handling, or emitted bytes from the real binary.
- One-line fix: add a focused integration assertion that invokes the built `extract` binary on the moved fixture paths.

### Verified clean

| Area | Evidence | Result |
|---|---|---|
| Diff identity | `gh pr view 65 --json headRefOid,baseRefOid` | base and head match the round brief |
| Overall diff count | summed `git diff --numstat` | `20` files, `591` additions, `583` deletions |
| Eight text baselines | complete-file substitution followed by `cmp` | all changes are the named path replacement |
| Binary SCIP fixtures | path-limited `git diff --name-only` | unchanged |
| New ignore reason text | zero-context diff of three test files | all five use the required literal reason |
| External binary failure | parent-state focused tests after `cargo build --features cli --bin extract` | all five stop on unavailable `rust-analyzer` |
| Source ownership inside tests, schema, reports | PR file list compared with stored brief | changes in those trees are within the declared set |
| Production source | PR file list | no `crates/sprefa-extract/src/**` file changed |

## Unexamined

- The clippy warning counts in the PR body were not reproduced.
- The two reported timing-flake histories were not reproduced.
- The three binary SCIP fixtures were checked for byte identity only; their encoded paths were not decoded.
- Documentation prose was checked for file scope and path substitution shape, not technical accuracy beyond this move.
- The full suite did not run past `104_tier_decline_diagnostic`; later test targets are unexamined in the PR-head suite.
- Correct expected content for the `534` changed wire-golden row pairs remains unexamined because no independent oracle was supplied.
