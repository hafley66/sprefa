# Soopy source mechanics boundary

## Context

Soopy lives at `~/projects/hafley-rs/crates/soopy` and supplies repository,
revision, path, content, snapshot, read, and watch coordinates without Sprefa
types. Sprefa currently consumes it through two adapters:

- `v6/sprefa-engine-rs/src/hosts.rs`: native execution for the existing
  `files`, `files_at`, `repo_files`, and `repo_files_at` host plans.
- `v6/sprefa-extract/src/project.rs`: `SourceTreeBlobSource`, which snapshots a
  revision once and reads the resulting entries through `BlobSource`.

The crate already binds these Rust libraries:

| Mechanic | Library |
|---|---|
| Ignore-aware traversal | `ignore` |
| Path glob compilation | `globset` |
| Filesystem events | `notify` |
| Worktree content identity | `blake3` |
| CLI and structured output | `clap`, `serde_json` |

Git discovery, revision resolution, index/tree enumeration, object hashing,
and object reads currently invoke the `git` executable. The CLI `query`
subcommand invokes `rg`; `--fzf` invokes `fzf`. The current watcher owns one
`Revision::Worktree` query, observes the worktree plus selected Git ref paths,
coalesces events for 120–600 ms, recomputes a complete snapshot, and emits
logical `SourceDelta` values.

The intended crate boundary includes filesystem traversal, Git repositories,
Git revisions, Git worktrees/checkouts, watches, search, and selection as typed
library mechanics. External executables remain available as adapters for
parity, compatibility, or terminal presentation.

## Decisions

1. Soopy owns source mechanics and types. Sprefa owns language, compiler,
   runtime, and relation semantics.
2. Filesystem, Git, search, and selection expose typed requests and results.
3. Rust library implementations back the normal API path.
4. The `git` executable remains an allowed source backend because Git owns its
   object database, index, refs, pathspec behavior, and worktree operations.
5. Text search uses the ripgrep Rust crates directly.
6. Fuzzy ranking uses high-level `nucleo`; `clap` exposes query, limit, output,
   and selection arguments without introducing a terminal UI framework.
7. `rg` and `fzf` subprocesses leave the normal CLI and library paths.
8. Worktree and immutable-revision snapshots share `SourceRef`, `SourceEntry`,
   `SourceBytes`, and `SourceDelta` coordinates.
9. Git worktree creation, removal, checkout attachment, HEAD movement, and ref
   movement receive typed events rather than collapsing into filesystem noise.
10. Content identity is declared per source surface and round-trips through the
   corresponding read operation.

Alternatives recorded for comparison:

- Shell pipelines outside Git as the library contract.
- A Sprefa-specific filesystem/Git host implementation.
- A terminal-specific `fzf` type inside the core data model.
- One global content identifier algorithm for worktrees and Git objects.

## Type signatures

```rust
pub trait SourceBackend {
    fn snapshot(&mut self, query: &SourceQuery) -> anyhow::Result<SourceSnapshot>;

    fn read_many(
        &mut self,
        requests: &[ReadRequest],
    ) -> anyhow::Result<Vec<SourceBytes>>;
}

pub trait SourceWatch {
    fn recv(&mut self) -> anyhow::Result<Vec<SourceDelta>>;

    fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Option<Vec<SourceDelta>>>;
}

pub trait SourceSearch {
    fn search(&mut self, query: &SearchQuery) -> anyhow::Result<Vec<SearchMatch>>;
}

pub trait SourceSelect<T> {
    fn rank(
        &mut self,
        query: &SelectionQuery,
        values: &[T],
    ) -> anyhow::Result<Vec<Selection>>;
}

pub trait RepositoryWatch {
    fn recv(&mut self) -> anyhow::Result<Vec<RepositoryDelta>>;
}

pub struct SearchQuery {
    pub sources: SourceQuery,
    pub matcher: SearchMatcher,
    pub limit: Option<usize>,
}

pub enum SearchMatcher {
    Literal(String),
    Regex(String),
    AstPattern { language: String, pattern: String },
}

pub enum RepositoryDelta {
    WorktreeAdded(WorktreeRef),
    WorktreeRemoved(WorktreeRef),
    CheckoutChanged { before: RevisionId, after: RevisionId },
    RefChanged { name: String, before: Option<ObjectId>, after: Option<ObjectId> },
    IndexChanged,
    RescanRequired,
}
```

Candidate native search stack:

```text
ignore
  → parallel repository traversal
grep-regex
  → regex compilation and matching
grep-searcher
  → buffered/memory-mapped searching and binary detection
grep-matcher
  → matcher interface shared by searchers
```

`fzf` is a Go terminal application. Soopy’s reusable layer therefore models
selection inputs, ranks, and selected identities. High-level `nucleo` owns
filtering and ranking for large candidate sets. `clap` exposes the request and
output controls. The core selection result contains stable identities and no
terminal state.

## Instance timelines

### Snapshot instance

1. A caller opens a repository coordinate.
2. `SourceQuery` selects a worktree or immutable revision and path patterns.
3. The backend resolves the revision once.
4. One traversal produces ordered `SourceEntry` values.
5. Parent directories derive from the selected file set.
6. The resulting `SourceSnapshot` lives until the caller replaces or drops it.

### Read instance

1. A caller derives `ReadRequest` values from snapshot entries.
2. The backend validates repository, revision, path, and expected content.
3. Worktree reads remain inside the canonical repository root.
4. Git reads validate `commit:path` against the expected blob identity.
5. Returned bytes carry the same source coordinate and content identity.

### Watch instance

1. A watch owns its repository session, query, cache, and prior snapshot.
2. Native filesystem and Git metadata events enter one bounded coalescing
   window.
3. Event classification identifies source, index, ref, checkout, and rescan
   causes.
4. The watch refreshes only the affected query state or performs an explicit
   complete rescan.
5. Logical deltas reference the before and after source coordinates.
6. Cache entries absent from the refreshed snapshot are released.

### Search and selection instance

1. Search consumes a source query and matcher.
2. Traversal and matching produce stable source spans.
3. Selection consumes those results without owning source enumeration.
4. Terminal adapters render and return selected stable identities.
5. Search and selection instances end when the request or interactive session
   ends.

## Storage, reads, writes, and uniqueness

`RepositoryId` determines repository equality across revisions and linked
worktrees. Its derivation must state whether it identifies a checkout, shared
Git object database, or configured logical repository. `WorktreeRef` identifies
one checkout separately.

`SourceRef` uniqueness is:

```text
(RepositoryId, RevisionId, RepoPath)
```

`RepoPath` stores a validated repository-relative path. Absolute paths,
root-escaping parent components, and lossy platform conversions are rejected or
represented losslessly before filesystem access.

`ContentId` identifies the bytes returned by `read_many`. Worktree BLAKE3 and
Git blob OIDs may coexist, but enumeration and reading must agree for each
`SourceEntry`.

The worktree cache stores metadata only for paths present in the latest
snapshot owned by that cache. Watch refreshes prune deleted and renamed paths.

Git object reads use a persistent batch process or a native object database
backend. A read validates the requested commit and path against the blob OID;
the OID alone cannot relabel unrelated bytes as another source coordinate.

Search results reference `SourceRef`, `ContentId`, and byte or line spans.
Selection results reference search-result identities and do not duplicate file
contents.

## Reviewed defects

<!-- todo(bug): Interpret `repo_files` and `repo_files_at` pathspecs relative to the selected repository root; only `files` and `files_at` use the process working directory. -->

<!-- todo(bug): Validate every worktree `RepoPath` before joining it to the repository root so absolute and parent-traversal paths cannot escape the repository. -->

<!-- todo(bug): Validate committed reads against `commit:path` before returning the caller-supplied expected blob. -->

<!-- todo(bug): Make `GitFilesQuery` worktree entries use a content identity that round-trips through `SourceTree::read_many`. -->

<!-- todo(bug): Prune deleted and renamed paths from `WorktreeCache` after every completed enumeration. -->

<!-- todo(bug): Define and test tracked symlink behavior consistently across worktree and immutable-revision snapshots. -->

<!-- todo(decision): Define whether `RepositoryId` identifies a checkout, shared Git object database, or configured logical repository, then test linked-worktree identity. -->

<!-- todo(bug): Check the exit status of `git status` before deriving a clean worktree revision. -->

## Sequence

1. Pin source-coordinate invariants and repair the reviewed integrity defects.
2. Split repository identity from worktree/checkout identity.
3. Introduce typed repository and Git-worktree discovery.
4. Extend watching with typed index, ref, checkout, worktree-add, and
   worktree-remove deltas.
5. Add typed search requests and results using the native Rust grep stack.
6. Add selection requests and results plus terminal adapters.
7. Remove `rg` and `fzf` subprocesses from Soopy's normal command paths.
8. Retain the `git` executable as an explicit Git backend.
9. Move Sprefa file hosts and extraction onto the stable interfaces.

<!-- todo(feature): Add typed repository, Git worktree, search, and selection interfaces to Soopy after source-coordinate invariants are fixed. -->

<!-- todo(feature): Implement native Rust text search over `SourceQuery` using the ripgrep library stack. -->

<!-- todo(feature): Replace the `fzf` subprocess with high-level `nucleo` ranking exposed through the existing `clap` command surface and stable selection identities. -->

<!-- todo(feature): Implement typed repository and checkout watches for worktree creation, removal, attachment, HEAD movement, ref movement, and index changes. -->

## Verification

Add deterministic tests for:

- repository-root and cwd-relative pathspec behavior;
- root-escaping and absolute read paths;
- mismatched commit, path, and expected blob identities;
- worktree `GitFilesQuery` enumeration followed by `read_many`;
- deletion and rename cache pruning;
- tracked symlinks in clean worktree and commit snapshots;
- linked worktree repository and checkout identities;
- failed Git commands;
- non-UTF-8 and newline-containing paths on supported platforms;
- worktree creation, removal, checkout, HEAD, ref, and index deltas;
- native-search parity with the selected `rg` semantics;
- stable selection identities across reordered result presentation.

Run:

```sh
cargo test -p soopy
cargo test --test live_hosts
cargo test
dl examples/gen-plans-index.dl --check
```

## Staffing

- Implementation: one Rust agent lane for Soopy mechanics and one Rust agent
  lane for Sprefa adapter parity after the Soopy API is pinned.
- Worktree: yes; isolate `hafley-rs` and `sprefa` changes into separate commits.
- Base SHA: record independently in each repository immediately before work.
- Suite budget: Soopy unit/integration tests on every change; Sprefa engine and
  extraction suites after adapter changes.
