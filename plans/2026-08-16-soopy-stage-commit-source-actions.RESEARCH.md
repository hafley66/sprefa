# Soopy stage/commit source actions

Research date: 2026-08-16

This document updates
[`2026-08-12-fs-effects-recon.RESEARCH.md`](2026-08-12-fs-effects-recon.RESEARCH.md)
after Soopy gained Git-optional source identities, immutable revision reads,
tracked-state queries, and repository watchers.

## Executive index

1. `stage` computes and validates a complete source transaction without changing
   target files.
2. `commit` applies one exact staged transaction after revalidating its input
   identities.
3. These names are source-control themed but do not require Git. A plain
   directory uses BLAKE3 content identities; a Git worktree adds repository,
   worktree, index, and HEAD observations.
4. Parser and codemod systems produce edits. Soopy groups, validates, previews,
   stages, and commits them.
5. DL6 owns relational policy: which findings become edits, which edits conflict,
   when a proposal is complete, and which approval fact releases it.
6. No surveyed Rust crate supplies this entire boundary. Published crates cover
   edit normalization, AST-specific mutation, unified diff rendering,
   capability-confined paths, per-file atomic replacement, or rollback as
   separate pieces.

## Terms

| word | meaning here |
|---|---|
| source | a file placement identified by root, revision/worktree when present, and relative path |
| action | one create, replace, move, or delete request |
| stage | normalize all actions, validate their expected source state, calculate outputs and preview, then return an immutable plan |
| commit | revalidate the staged plan and perform its filesystem operations |
| Git commit | creation of a Git commit object; outside this API |
| proposal | relational representation of a staged plan presented to a human or policy engine |
| approval | an input fact naming the exact staged-plan digest allowed to commit |
| receipt | the typed result of commit, including before/after identities and every applied operation |

`commit` is usable on a directory with no `.git`. The operation commits a
staged source transaction to the filesystem. It neither stages Git's index nor
creates a Git commit.

## The system boundary

```text
Biome rule action ─────┐
ast-grep edit ─────────┤
Rust analyzer assist ──┤
DL6-native rule ───────┼──> SourceAction rows
V5 move rewriter ──────┘          |
                                  v
                         Soopy::stage(actions)
                                  |
                     validate, group, splice, diff
                                  |
                                  v
                    StagedSourceTransaction
                    { id, inputs, outputs, diff }
                                  |
                                  v
                 DL6 proposed_action / proposed_file
                                  |
                     awaiting_approval(stage_id)
                                  |
                    approve(stage_id) arrives
                                  |
                                  v
                         Soopy::commit(stage)
                                  |
                    revalidate, apply, observe
                                  |
                                  v
                           CommitReceipt
```

The dataflow reaches quiescence with proposal rows present. It does not keep a
Timely/Differential frontier open while a person thinks. Approval is a later
input tick. This preserves deterministic proposal derivation and prevents a
slow user decision from blocking unrelated dataflow work.

## Version archaeology

The full file and line receipts are in
[`2026-08-12-fs-effects-recon.RESEARCH.md`](2026-08-12-fs-effects-recon.RESEARCH.md).

| generation | source acquisition | edit representation | grouping and conflict handling | preview/approval | filesystem application | feedback |
|---|---|---|---|---|---|---|
| V3 | `FsListFilesEffect`, scalar and batched byte reads | `WriteFileEffect`, `WriteRangeEffect`, `MutationEffect` | read batching existed; write fan-in did not | `WritePolicy`, `WriteDecision`, staged rows, `SubjectRegistry<WriteApproval>` | sink selected disk, buffer, or stage-and-approve | recorded staged result; filesystem cache invalidation |
| V4 | component/runtime reads | cursor byte ranges | grouped by file, sorted right-to-left, one read and one write per file | no general dry-run surface | direct component I/O | drift diagnostic plus dirty-source dispatch |
| V5 | repository engine, Git relations, located spans | `refactor::Edit { path, lo, hi, old_text, new_text }` | `group_by_file`, descending splice, overlap rejection | `--move` dry-ran by default; `--fix` applied | rewrote Rust/Kotlin imports, moved files, repaired Rust `mod` declarations; verify journal restored bytes on checker failure | rescan and write ledger |
| V6 TS lab | host/bind reads, staged rows | `edit_add` and `edit_del` relations | one host call per row in the measured lab | `armed(zone)` was the approval fact | subprocess helper rewrote the file | watcher could provide the new digest; POST-only execution remained blind |
| Soopy now | Git-optional snapshots, verified batch reads, source spans, watchers | no write action type | no edit grouping | no stage/commit plan | no write API | source, index, ref, and worktree deltas only |

### What should survive

| retained mechanic | source |
|---|---|
| stable per-action decision rows | V3 |
| explicit yield/resume approval | V3 |
| group edits by source and splice original-coordinate ranges right-to-left | V4/V5 |
| reject overlapping ranges before touching disk | V5 |
| compare expected old text/content before applying | V5 plus Soopy `ContentId` |
| dry-run as the staged data itself | V6 lab |
| approval as a later ordinary fact | V6 lab |
| Git-optional root, worktree, revision, file, span, and byte identities | Soopy |
| rename-aware watch receipts after application | Soopy |

## Soopy capability delta

### Present

| primitive | current Soopy type/API | use in source actions |
|---|---|---|
| generic directory root | `DirectoryRoot` | stage and commit outside Git |
| repository identity | `RepositoryId` | prevent cross-repository plan reuse |
| checkout identity | `WorktreeId` | distinguish three checkouts of the same repository |
| revision identity | `RevisionId::{Worktree, Commit}` | immutable reads and mutable target selection |
| path identity | `RepoPath` | root-relative action addressing |
| content identity | `ContentId::{GitBlob, Blake3}` | optimistic concurrency precondition |
| source identity | `SourceRef` | file placement at one revision/worktree |
| byte range | `SourceSpan` and `span_slice` | locate one replacement |
| source enumeration and reads | `SourceTree::snapshot`, `read_each`, `read_many` | stage against verified bytes |
| Git state | tracked-state query, index/ref/worktree snapshots | report surrounding source-control state |
| live invalidation | directory/source/repository watchers | observe committed actions and feed new facts |

### Missing

| primitive | required behavior |
|---|---|
| `TextEdit` | original-file byte range plus replacement bytes/text |
| edit batch | group by source, deterministic ordering, overlap/duplicate refusal |
| source actions | create, replace, move, delete |
| preconditions | expected absent/present content, expected file kind and optional mode |
| staged transaction | immutable normalized action set plus before/after identities and digest |
| preview | deterministic per-file unified diff plus move/create/delete summary |
| staging store | retain replacement bytes across a human pause or process restart |
| commit engine | revalidate all inputs, materialize outputs, apply operations, return receipt |
| recovery | report and recover a partially applied multi-file transaction |
| CLI | `soopy stage`, `soopy commit`, `soopy show-stage`, `soopy discard-stage` |

## Proposed Rust types

Files should follow Soopy's numeric reading order. Foundational action types
belong after current source/span types and before the planner and committer.

```rust
pub struct TextEdit {
    pub range: SourceSpan,
    pub replacement: Vec<u8>,
    pub producer: ActionProducer,
}

pub enum SourceAction {
    Create {
        path: RepoPath,
        bytes: StagedContent,
    },
    Replace {
        source: SourceRef,
        expected: ContentId,
        edits: Vec<TextEdit>,
    },
    Move {
        source: SourceRef,
        expected: ContentId,
        destination: RepoPath,
    },
    Delete {
        source: SourceRef,
        expected: ContentId,
    },
}

pub struct StageRequest {
    pub root: SourceRootId,
    pub actions: Vec<SourceAction>,
}

pub struct StagedSourceTransaction {
    pub id: StageId,
    pub root: SourceRootId,
    pub files: Vec<StagedFile>,
    pub preview: Vec<FilePreview>,
}

pub struct StagedFile {
    pub path_before: Option<RepoPath>,
    pub path_after: Option<RepoPath>,
    pub content_before: Option<ContentId>,
    pub content_after: Option<ContentId>,
    pub staged_bytes: Option<StagedContentId>,
}

pub enum CommitOutcome {
    Committed(CommitReceipt),
    Stale(Vec<StaleInput>),
    Refused(Vec<ActionConflict>),
    RecoveryRequired(RecoveryReceipt),
}
```

`StageId` hashes the root identity, normalized action sequence, expected input
identities, resulting content identities, and staged-content identifiers. It
does not hash presentation-only diff formatting.

## Byte-span edit grouping

Soopy cannot perform this today. The required algorithm is:

1. group `TextEdit` values by `SourceRef`;
2. require every edit in a group to name the same expected `ContentId`;
3. sort by `(start, end, producer identity)` for canonical validation;
4. reject intersecting ranges, duplicate insertions at one offset without an
   explicit producer order, ranges outside the input, and ranges whose source
   identity does not match the target root;
5. apply accepted edits against the original bytes from highest offset to
   lowest offset;
6. hash the final bytes and store them once;
7. emit one `StagedFile` and one preview per file.

Adjacent ranges are legal. Multiple zero-width insertions at one offset require
an explicit order because sorting them silently changes generated source.

For UTF-8 code edits, [`ra_ap_text_edit` 0.0.241](https://docs.rs/ra_ap_text_edit/latest/ra_ap_text_edit/)
already provides `Indel`, `TextEditBuilder`, sorted/disjoint edit invariants,
`union`, and application. Its offsets use `TextSize` and its replacement is a
`String`. Soopy still needs a byte-oriented whole-file action for generated or
non-UTF-8 content. An adapter can map UTF-8 `TextEdit` values into Soopy's
serialized action type.

## Stage lifecycle

```text
Unstaged request
  -> normalize paths
  -> snapshot every input
  -> read each input once
  -> validate old content/ranges
  -> group and apply edits in memory
  -> detect path and move conflicts
  -> hash every result
  -> persist result blobs in StageStore
  -> derive preview
  -> seal StageId
  -> Staged
```

`stage` may write to a caller-selected staging store. It does not mutate target
files, the Git index, refs, or commits. A persistent store is required when a
proposal must survive process restart. An in-memory store is sufficient for an
LSP code action that remains in one process.

## Commit lifecycle

```text
Staged
  -> acquire root-scoped writer lock
  -> re-snapshot every touched input and destination
  -> compare all preconditions
  -> refuse the entire plan if any input is stale
  -> prepare same-directory replacement files
  -> write a recovery journal
  -> replace/create/move/delete in canonical order
  -> snapshot resulting paths
  -> mark journal committed
  -> CommitReceipt
```

Portable filesystems do not provide a general atomic transaction across several
paths. Each replacement can be atomic while the batch remains recoverable rather
than globally atomic. The recovery journal must therefore describe which
operations crossed the visibility boundary.

## Git-optional behavior

| concern | plain directory | Git worktree |
|---|---|---|
| root identity | canonical directory identity | `RepositoryId` plus `WorktreeId` |
| source content | BLAKE3 | worktree BLAKE3; immutable commit sources retain Git blob OIDs |
| mutable target | directory root | exact worktree; committed revisions are read-only inputs |
| stale detection | compare BLAKE3 and presence | compare content plus worktree identity; optionally report index/HEAD transition |
| move | filesystem rename | filesystem rename only; Git observes it afterward |
| stage meaning | Soopy source-action stage | same; does not run `git add` |
| commit meaning | apply source transaction | same; does not run `git commit` |
| post-state | directory snapshot/delta | source delta plus tracked/staged/unstaged classification |

Git remains an observation and immutable-object backend. Source-action commit
does not modify the Git index unless a later, separately named API is designed.

## DL6 relational surface

```dl6
rel source_action(
  proposal: text,
  producer: text,
  path: text,
  start_byte: int,
  end_byte: int,
  expected_content: text,
  replacement: text
).

rel action_conflict(proposal: text, left: text, right: text, reason: text).
rel staged_file(proposal: text, path: text, before: text, after: text, diff: text).
rel awaiting_approval(proposal: text, stage_id: text).
rel approval(stage_id: text).
rel committed_file(stage_id: text, path: text, before: text, after: text).

ready(Stage_id) <-
  awaiting_approval(_Proposal, Stage_id),
  approval(Stage_id),
  not(action_conflict(_Proposal, _, _, _)).
```

The runtime calls Soopy `stage` after the edit-producing relations settle. It
publishes `staged_file` and `awaiting_approval`. A later `approval(StageId)`
arrival enables the commit host. Approval names the sealed digest displayed to
the user.

## Biome and ast-grep assimilation

### Common producer envelope

```rust
pub struct ProducedEdit {
    pub producer: ActionProducer,
    pub source: SourceRef,
    pub expected: ContentId,
    pub start_byte: u64,
    pub end_byte: u64,
    pub replacement: Vec<u8>,
    pub finding: Option<ExternalFindingId>,
}
```

Each engine remains responsible for parsing and generating a syntactically
valid replacement. Soopy receives the common envelope.

### ast-grep

[`ast-grep-core` 0.45.0](https://docs.rs/ast-grep-core/latest/ast_grep_core/)
provides parsing, matching, traversal, and replacers. Its edit model is the
same essential tuple: byte range plus inserted content. The adapter runs a rule,
turns each match/fix into `ProducedEdit`, and stops before CLI application.

DL6 can then:

- join ast-grep findings with Git state, ownership, dependency, or type facts;
- suppress or prioritize edits relationally;
- combine edits from several rules;
- expose conflicts before application;
- stage one cross-language proposal;
- approve and commit the sealed proposal once.

### Biome

Biome's published [`BatchMutation`](https://docs.rs/biome_rowan/latest/biome_rowan/struct.BatchMutation.html)
collects syntax-node/token mutations and can return a text range plus a
`TextEdit` when committed. It is language/CST-specific and is neither `Send`
nor `Sync`. The adapter runs inside the Biome language worker, converts the
resulting text edits to `ProducedEdit`, and releases its syntax tree before the
Soopy stage.

Custom Biome lint rules can be assimilated in three increments:

1. retain the Biome rule and ingest its diagnostic/action output;
2. move cross-rule selection, joins, severity, ownership, and approval into DL6;
3. migrate the match itself only when DL6's CST/type facts express the same
   condition and the replacement producer has an equivalent test oracle.

This avoids coupling Soopy to Biome's syntax tree while allowing DL6 to become
the policy and composition layer.

### rust-analyzer assists

Rust-analyzer's `SourceChange` groups per-file `TextEdit` values and filesystem
edits. Its published `ra_ap_text_edit` crate supplies the reusable text-edit
algebra; the complete `SourceChange` and semantic assist machinery remain tied
to rust-analyzer. A Rust-assist adapter can emit the same `ProducedEdit` and
move/create operations.

### GritQL and codemod tools

[`GritQL`](https://github.com/biomejs/gritql) and
[`Codemod CLI`](https://github.com/codemod/codemod) are polyglot transformation
systems with their own rule/workflow runtimes and first-class ast-grep support.
They can serve as external edit producers. Importing their complete runtimes is
separate from the Soopy source-transaction boundary.

## Rust library survey

Versions were verified on 2026-08-16 from official repositories or docs.rs.

| crate/system | verified version | reusable capability | boundary it does not supply |
|---|---:|---|---|
| [`ra_ap_text_edit`](https://docs.rs/ra_ap_text_edit/latest/ra_ap_text_edit/) | 0.0.241 | disjoint sorted UTF-8 text edits, builder, union, apply | file identities, persistence, moves, preview, transaction |
| [`ast-grep-core`](https://docs.rs/ast-grep-core/latest/ast_grep_core/) | 0.45.0 | polyglot tree-sitter matching and replacement generation | cross-producer transaction and approval |
| [`biome_rowan::BatchMutation`](https://docs.rs/biome_rowan/latest/biome_rowan/struct.BatchMutation.html) | 0.5.8 published crate | CST mutation batch and derived text edit | generic filesystem transaction; tied to Biome language trees |
| [`diffy`](https://docs.rs/diffy/latest/diffy/) | 0.5.x | byte/text patch creation, unified format, application, three-way merge, multi-file patch parsing | source preconditions and filesystem transaction |
| [`atomic-write-file`](https://docs.rs/atomic-write-file/latest/atomic_write_file/) | 0.3.0 | crash-tested atomic replacement of one file on major platforms | multi-file commit, moves/deletes, approval |
| [`cap-std`](https://docs.rs/cap-std/latest/cap_std/fs/) | 4.0.2 | root-confined relative filesystem operations | edit algebra and transaction |
| [`cap-std-ext`](https://docs.rs/cap-std-ext/latest/cap_std_ext/dirext/) | 5.1.2 | capability directory helpers and atomic write methods | multi-file recovery protocol |
| [`fs_rollback`](https://docs.rs/fs_rollback/latest/fs_rollback/) | 3.0.1 | rollback wrapper for common filesystem operations | Soopy identities, edit grouping, DL6 rows; crash/restart guarantees require a spike |
| [`codemod-core`](https://docs.rs/codemod-core/latest/codemod_core/) | 0.1.2 | example-driven TypeScript/JavaScript scanning, preview, conflict and rollback APIs | current roadmap marks Rust/Go/Python and incremental Git scanning as future work |

### Composition selected for the Soopy spike

| layer | candidate to test | receipt required |
|---|---|---|
| UTF-8 edit algebra | `ra_ap_text_edit` adapter | V5 edit fixtures produce byte-identical outputs; overlaps and same-offset inserts refuse deterministically |
| preview | `diffy` | stable create/modify/move/delete display over pinned fixtures |
| path confinement | `cap-std` | traversal, symlink, and cross-root destinations refuse |
| per-file visibility | `atomic-write-file` or `cap-std-ext` | kill during write leaves old or new complete bytes |
| multi-file recovery | `fs_rollback` evaluation versus a Soopy journal | injected failure after each operation recovers or yields an exact recovery receipt; restart behavior measured |
| structural edit production | existing `ast-grep-core`; Biome adapter when used | edits enter the same `ProducedEdit` fixture as native DL6 edits |

No selected library owns the public Soopy types. Library-specific edits are
converted at adapters so the serialized stage remains stable if a dependency
changes.

## CLI surface

```text
soopy stage --root PATH --actions actions.jsonl --store STAGE_DIR
soopy show-stage STAGE_ID --store STAGE_DIR --format json|diff
soopy commit STAGE_ID --store STAGE_DIR
soopy discard-stage STAGE_ID --store STAGE_DIR
```

`stage` prints one sealed JSON document containing `stage_id`, conflicts,
per-file before/after identities, and preview. A conflict exits nonzero and does
not seal a committable plan. `commit` refuses a stale plan and prints the exact
stale paths and observed identities.

## Required correctness matrix

| case | stage | commit |
|---|---|---|
| two disjoint edits in one file | one grouped output | one file replacement |
| overlapping edits | conflict | unavailable |
| adjacent edits | accepted | exact bytes |
| two unordered inserts at one offset | conflict | unavailable |
| same edit from two producers | deduplicated with both provenances | one replacement |
| target changes after stage | plan remains inspectable | stale refusal, zero writes |
| destination appears after move stage | plan remains inspectable | stale refusal, zero writes |
| plain directory | BLAKE3 identities | commit without Git |
| three worktrees of one repository | worktree-qualified stage | only selected worktree changes |
| immutable commit source | readable as input | cannot be mutation target |
| failure during one file replacement | sealed plan retained | complete old or new file |
| failure midway through several paths | sealed plan retained | recovery receipt names crossed operations |
| identical re-commit | same stage digest | idempotent receipt or already-committed result |
| watcher active during commit | normalized logical deltas | no feedback loop reapplying the stage |

## Implementation slices

```text
0 source-action types and serialized schema
1 UTF-8 edit grouping adapter over ra_ap_text_edit
2 stage planner with content/path preconditions
3 deterministic diff preview
4 persistent StageStore and sealed StageId
5 root-confined commit engine and per-file atomic replacement
6 multi-file recovery journal and fault-injection matrix
7 Git/worktree post-state receipt
8 clap stage/show-stage/commit/discard-stage commands
9 ast-grep ProducedEdit adapter
10 Biome action adapter
11 DL6 proposed_action/approval/receipt host boundary
12 V5 move-refactor parity fixture
```

Slices 0 through 8 belong to Soopy. Slices 9 and 10 are producer adapters.
Slice 11 belongs to the DL6 runtime integration. Slice 12 proves the combined
system against the old import/module move behavior.

## Source inventory

Local primary sources:

- Soopy: `~/projects/hafley-rs/crates/soopy/src`, `README.md`, and tests.
- V3/V4 archives: paths and symbols cataloged in
  [`2026-08-12-fs-effects-recon.RESEARCH.md`](2026-08-12-fs-effects-recon.RESEARCH.md).
- V5 edit algebra: `src/refactor.rs`, `src/lib.rs::run_move`,
  `tests/it/move_refactor.rs`, `book/tutorial/10-move-a-file.md`.
- V6 staged-write measurements: `v6/tsv2/labs/staged-writes` and
  `plans/2026-07-30-staged-writes-lab.md`.

External primary sources:

- [rust-analyzer repository and architecture](https://github.com/rust-lang/rust-analyzer)
- [`ra_ap_text_edit` API](https://docs.rs/ra_ap_text_edit/latest/ra_ap_text_edit/)
- [`ast-grep-core` API](https://docs.rs/ast-grep-core/latest/ast_grep_core/)
- [ast-grep replacement API](https://astgrep.com/reference/api.html)
- [`Biome BatchMutation` API](https://docs.rs/biome_rowan/latest/biome_rowan/struct.BatchMutation.html)
- [GritQL repository](https://github.com/biomejs/gritql)
- [Codemod CLI repository](https://github.com/codemod/codemod)
- [`diffy` API](https://docs.rs/diffy/latest/diffy/)
- [`atomic-write-file` API](https://docs.rs/atomic-write-file/latest/atomic_write_file/)
- [`cap-std` filesystem API](https://docs.rs/cap-std/latest/cap_std/fs/)
- [`cap-std-ext` atomic directory helpers](https://docs.rs/cap-std-ext/latest/cap_std_ext/dirext/)
- [`fs_rollback` API](https://docs.rs/fs_rollback/latest/fs_rollback/)
- [`codemod-core` API](https://docs.rs/codemod-core/latest/codemod_core/)

## Open measurements

1. `ra_ap_text_edit` dependency and serialization adapter size in Soopy.
2. `fs_rollback` behavior after process death at every commit step.
3. `atomic-write-file` metadata preservation requirements for executable and
   read-only source files.
4. rename behavior across mount points and case-only renames on macOS/Windows.
5. watcher suppression or receipt correlation for Soopy's own commits.
6. persistent staging-store location and garbage-collection policy.
7. unified diff representation for binary files and path moves.

