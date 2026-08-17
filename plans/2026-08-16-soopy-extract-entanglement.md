# soopy / sprefa-extract / sprefa-engine-rs / dl6 hosting: measured entanglement

Survey date 2026-08-16, grep-backed, every claim cited. Condensed board:
this doc's twin conversation. Full six sections below.

## TOC
1. Dependency edges (4 consumers, 2 lockfiles, transitive skew)
2. Consumed soopy surface (extract is a strict subset of engine; 58% dead)
3. Type twins (BlobHash vs ContentId; FsBlobSource vs SourceTreeBlobSource; stringly revs)
4. dl6 hosting seam (7 executor families, who reads through what, memo keys)
5. Worktree visibility per host
6. Extract's 13 read paths (2 through BlobSource, 1 rev-pinned, 0 production callers)
7. Collapse candidates, ranked
I have everything I need. Here is the measured map.

---

# 1. DEPENDENCY EDGES

## 1.1 Declared path deps

| Consumer | Manifest | Line | Dep declaration |
|---|---|---|---|
| sprefa-extract | `/Users/chrishafley/projects/sprefa/v6/sprefa-extract/Cargo.toml` | 92 | `soopy = { path = "../../../hafley-rs/crates/soopy" }` |
| sprefa-engine-rs | `/Users/chrishafley/projects/sprefa/v6/sprefa-engine-rs/Cargo.toml` | 22 | `soopy = { path = "../../../hafley-rs/crates/soopy" }` |
| sprefa-engine-rs | same | 18 | `sprefa-extract = { path = "../sprefa-extract" }` |
| sprefa-engine-rs | same | 19 | `sprefa-rust-runtime-host = { path = "../../../sprefa-v6/0_runtime/1_rust_runtime_host" }` |
| **sprefa-rust-runtime-host** (3rd consumer, different repo) | `/Users/chrishafley/projects/sprefa-v6/0_runtime/1_rust_runtime_host/Cargo.toml` | 9 | `soopy = { path = "../../../hafley-rs/crates/soopy" }` |
| **sprefa-source-identity-store** (4th consumer) | `/Users/chrishafley/projects/sprefa-v6/0_runtime/0_source_identity_store/Cargo.toml` | 11 | `soopy = { path = "../../../hafley-rs/crates/soopy" }` |

There are **four** soopy consumers in the sprefa constellation, not two. The engine reaches soopy three ways: directly (Cargo.toml:22), through sprefa-extract (:18), and through sprefa-rust-runtime-host → sprefa-source-identity-store (:19).

## 1.2 Back edges

**None.** `grep -rn "sprefa" --include=*.rs --include=*.toml /Users/chrishafley/projects/hafley-rs` returns 17 hits, all string literals in `crates/boop` (`crates/boop/src/lane.rs:641`, `crates/boop/src/main.rs:2856`, `crates/boop/src/channel/tui.rs:637`, …) naming a tmux lane `"sprefa-coordinator"`. Zero hits in `crates/soopy`. soopy has no knowledge of sprefa.

## 1.3 Workspace isolation and version skew

Both consumers declare their own `[workspace]` table (`sprefa-extract/Cargo.toml:4`, `sprefa-engine-rs/Cargo.toml:1`), so each has its **own lockfile**: `v6/sprefa-extract/Cargo.lock` and `v6/sprefa-engine-rs/Cargo.lock`. soopy itself is `soopy 0.1.0` in both (`sprefa-extract/Cargo.lock:1108`, `sprefa-engine-rs/Cargo.lock:1408`) with an identical dep list, but its **transitives are resolved to different versions**:

| soopy transitive | extract lock | engine lock | skew |
|---|---|---|---|
| blake3 | 1.8.5 | 1.8.6 | yes |
| globset | 0.4.19 | 0.4.20 | yes |
| ignore | 0.4.31 | 0.4.33 | yes |
| clap | 4.6.4 | 4.6.6 | yes |
| notify | 8.2.0 | 8.2.0 | no |
| notify-debouncer-full | 0.7.0 | 0.7.0 | no |
| sysinfo | 0.39.6 | 0.39.6 | no |
| serde / serde_json / anyhow | 1.0.229 / 1.0.151 / 1.0.104 | identical | no |

`ignore` 0.4.31 vs 0.4.33 is the load-bearing one: `ignore::WalkBuilder` is what decides worktree membership at `/Users/chrishafley/projects/hafley-rs/crates/soopy/src/_4_worktree.rs:36`. Two builds of the same soopy source can disagree on which files exist.

**Feature skew: none.** Neither consumer passes `features`/`default-features` to soopy; soopy declares no `[features]` table at all (`soopy/Cargo.toml` has no `[features]`).

---

# 2. SOOPY'S PUBLIC SURFACE CONSUMED

soopy's export set is `pub use _0_types::*` + `pub use _7b_source_actions::*` + 13 explicit re-exports (`soopy/src/lib.rs:23-35`). That is **108 exported items**. Raw density: `soopy::` appears **10 times** in `sprefa-extract/src` and **105 times** in `sprefa-engine-rs/src`.

## 2.1 Per-consumer inventory

sprefa-extract touches soopy in exactly **two files**:

| File:line | soopy items |
|---|---|
| `sprefa-extract/src/lib.rs:54-56` | re-exports `ContentId`(→`SourceContentId`), `Pattern`(→`SourcePattern`), `ReadRequest`(→`SourceReadRequest`), `RepositoryId`(→`SourceRepositoryId`), `Revision`(→`SourceRevision`), `RevisionId`(→`SourceRevisionId`), `SourceEntry`, `SourceRef` |
| `sprefa-extract/src/project.rs:630-673` | `SourceTree`, `SourceEntry`, `Revision`, `Pattern`, `discover`, `SourceQuery`, `ReadRequest`, `SourceTree::open/snapshot/read_many` |

sprefa-engine-rs touches soopy in **six files**: `hosts.rs` (25), `source_bind/_1_runtime.rs` (30), `source_bind/_0a_inputs.rs` (20), `dep_resolve.rs` (11), `change_facts.rs` (9), `source_bind/_0_types.rs` (2).

## 2.2 Used by BOTH (measured `soopy::X` occurrences, src only; extract's brace-group re-export at lib.rs:54 counted)

| Item | extract | engine | engine sites |
|---|---|---|---|
| `SourceTree` | 2 | 6 | `hosts.rs:158`, `change_facts.rs:74,156`, `dep_resolve.rs:430,434,442` |
| `Revision` | re-export + `project.rs:637` | 7 | `hosts.rs:148,149`, `change_facts.rs:77`, `dep_resolve.rs:454,476` |
| `ReadRequest` | 1 (`project.rs:666`) | 6 | `_0a_inputs.rs:71,78,79`, `_1_runtime.rs:215`, `dep_resolve.rs:482,484` |
| `ContentId` | re-export | 7 | `hosts.rs:171`, `change_facts.rs:83`, `_1_runtime.rs:30,347,359,372,385` |
| `RepositoryId` | re-export | 10 | `_0a_inputs.rs:8,19,79`, `_1_runtime.rs:28,60,324,348,360,373,387` |
| `RevisionId` | re-export | 7 | `_0a_inputs.rs:82,86`, `_1_runtime.rs:329,331,332`, `dep_resolve.rs:456,457` |
| `SourceRef` | re-export | 7 | `_0_types.rs:134`, `_1_runtime.rs:210,240,323,346,358,384` |
| `SourceEntry` | 2 (`project.rs:631,658`) | 0 in src (2 in tests) | — |
| `Pattern` | 1 (`project.rs:638`) | 0 in src (1 in tests) | — |
| `discover` | 1 (`project.rs:640`) | 4 | `hosts.rs:157,443,560`, `change_facts.rs:153` |
| `SourceQuery` | 1 (`project.rs:643`) | 0 in src (1 in tests) | — |

## 2.3 Used ONLY by sprefa-engine-rs

`GitFilesQuery` (3: `hosts.rs:161`, `change_facts.rs:76`, `dep_resolve.rs:478`) · `GitFileQuery` (2) · `ObjectId` (3: `change_facts.rs:199,204`, `dep_resolve.rs:476`) · `GitBatch` (1: `change_facts.rs:193`) · `SourceRoot` (4) · `GitWorktreeRoot` (1) · `DirectoryRoot` (2) · `DirectoryId` (5) · `WorktreeId` (6) · `FileQuery` (2) · `FileSnapshot` (2) · `TrackedStateResult` (2) · `SourceBytesRef` (1) · `SourceSpan` (2) · `Refs` (1: `hosts.rs:452`) · `RefQuery` (1) · `RefSnapshot` (2) · `RefObservation` (1) · `RevisionGraph` (1: `hosts.rs:563`) · `RevisionGraphQuery` (3) · `RevisionGraphResult` (2) · `RevisionResolution` (4).

## 2.4 Used ONLY by sprefa-extract

**None.** Every soopy item extract touches is also touched by the engine. Extract's soopy surface is a strict subset.

## 2.5 DEAD SURFACE (zero hits in either consumer's `src/`)

**63 of 108 exported items, 58%.** Grouped by module:

| soopy module | Dead exports |
|---|---|
| `_13_fetch` / acquisition | `Acquisition`, `AcquisitionPolicy`, `AcquisitionOperation`, `AcquisitionRequest`, `AcquisitionReceipt`, `AcquisitionOutcome` (6/6 — **module 100% dead**) |
| `_7b_source_actions` | `SOURCE_ACTION_SCHEMA_VERSION`, `SourceRootId`, `SourcePath`, `ActionSource`, `ActionSpan`, `ActionProducer`, `TextEdit`, `Utf8TextEdit`, `SourceAction`, `StageRequest`, `SourceActionValidationError` (11/11 — **module 100% dead**) |
| `_8_watch` / `_8a_watch_core` | `DirectoryWatcher`, `RepositoryWatcher`, `SourceWatcher`, `WatchQuery`, `WatchCoalescing`, `FileWatchQuery`, `SourceTree::watch`, `SourceTree::watch_repository` (**100% dead**; `.watch(` = 0 in both) |
| deltas | `SourceDelta`, `DirectoryDelta`, `RefDelta`, `IndexDelta`, `WorktreeDelta`, `RepositoryDelta`, `diff_refs` (7 — **the entire incremental/delta API is unused**) |
| `_7a_spans` | `span_slice`, `SpanText`, `SpanTextRequest`, `SpanPosition`, `SpanPositionRequest`, `BytePosition` (6 — engine holds `SourceSpan` but never asks soopy to resolve it) |
| `_5a_git_status` tracked-state detail | `TrackedFileObservation`, `TrackedFileState`, `TrackedFileUnsupported`, `TrackedHeadState`, `TrackedStateMetrics`, `EntryIdentity`, `EntryTransition`, `IndexStageEntry`, `GitEntryKind`, `GitEntryMode`, `UntrackedFilePolicy` (11 — engine calls `tracked_state` at `_0a_inputs.rs:66` but only names the wrapper `TrackedStateResult`) |
| filesystem-first | `FileRef`, `FileEntry`, `FileReadRequest`, `FileBytesRef`, `RootPath`, `DirectoryEntry` (6) |
| revision-graph detail | `CommitParents`, `Ancestry`, `MergeBase`, `AheadBehind`, `CommitWalk` (5 — reached through `RevisionGraphResult` fields, never named) |
| snapshots / misc | `SourceSnapshot`, `IndexSnapshot`, `IndexId`, `WorktreeSnapshot`, `WorktreeObservation`, `RepositorySnapshot`, `HeadObservation`, `Head`, `ObjectKind`, `Tagger`, `TagMetadata`, `RefId`, `RepoPath`, `Repository`, `SourceBytes`, `open` (16 — `open` used only in an engine *test*, `tests/source_bind/_0_runtime.rs:84`) |

Notable: `SourceTree::enumerate` is exported and public but the raw `.enumerate(` counts (19 extract / 27 engine) are **all iterator `.enumerate()`**, not `SourceTree::enumerate` — that method is reached only indirectly via `snapshot` (`soopy/src/_7_source_tree.rs:61`).

---

# 3. TYPE TWINS / REDEFINITIONS

## 3.1 `BlobHash` vs `ContentId::Blake3`

| Side | Cite | Shape |
|---|---|---|
| sprefa-extract | `src/types.rs:55` `pub struct BlobHash(pub [u8; 16]);` | blake3 **truncated to 16 bytes** (`types.rs:59-64`), hex via `to_hex` (`types.rs:67`) |
| soopy | `src/_0_types.rs:266-269` `enum ContentId { GitBlob(ObjectId), Blake3([u8;32]) }` | **full 32-byte** blake3, produced at `_4_worktree.rs:81` |

Both call `blake3::hash` on the same bytes; extract discards the top 16 bytes and drops the git-blob alternative entirely. `BlobHash` has **82 references** across `sprefa-extract/src` + `tests`, including load-bearing structural positions: `ProjectEdge.dst_blob` (`types.rs:753`), `DefSite.blob` (`types.rs:914`), the phase-1/phase-2 cache keys (`types.rs:1097-1098`), the wire digest (`wire.rs:240` `BlobHash::of(content).to_hex()`), and every language arm's join key (`lang/rust.rs:756,849,861,887,892`; `lang/go.rs:1526,1624,1636,1664,1669`; `lang/kotlin.rs:44`). Extract carries its own `blake3 = "1"` dep for this (`Cargo.toml:89`) on top of soopy's.

Consequence: a `ContentId::GitBlob` from `SoopyFilesExecutor` and a `BlobHash` from `wire.rs:240` are **not comparable**, so the digest that flows through the dl6 host plan cannot be checked against extract's own content key.

## 3.2 `FsBlobSource` vs `SourceTreeBlobSource`

Two impls of one trait (`sprefa-extract/src/types.rs:839` `pub trait BlobSource: Sync + Send`):

| Impl | Cite | Read mechanism |
|---|---|---|
| `FsBlobSource` | `project.rs:623-625` (def), `project.rs:689-692` (impl) | `std::fs::read(self.root.join(path)).ok()` — raw disk, **no revision at all** |
| `SourceTreeBlobSource` | `project.rs:629-632` (def), `project.rs:663-674` (impl) | `soopy::SourceTree::read_many` with `expected: Some(entry.content)` — revision-pinned with content verification |

**Call-site census:** `FsBlobSource` is used at `project.rs:147` (`resolve_project` reader) and `project.rs:195` (`scip_facts` reader) — the only two production readers. `SourceTreeBlobSource` has **zero production call sites**: `grep -rn SourceTreeBlobSource src tests` = `project.rs:629/634/663` (its own definition), `lib.rs:52` (re-export), and `tests/10_source_tree.rs:4,44`. The rev-correct implementation exists, is tested (`tests/10_source_tree.rs:42-57` proves it reads `VERSION: u8 = 1` from HEAD while the worktree holds `= 2`), and **nothing in the crate calls it**.

The doc comment at `project.rs:610-614` admits the duplication: *"`BlobSource` shipped as a trait with no implementation anywhere in the crate, which is why every caller that needed a rev-correct reader wrote the same closure by hand (this module did it twice before this type existed…)"*.

## 3.3 Rev/revision string plumbing bypassing `soopy::Revision`

Every one of these carries a revision as `&str`/`String` and re-wraps at the leaf instead of threading `soopy::Revision`:

| Site | Cite | Bypass |
|---|---|---|
| `SoopyFilesExecutor` | `hosts.rs:149-153` | `env.get("rev")` → `Revision::Named(Arc::from(str))`. Rev arrives as an untyped shell env var. |
| `IRevisionDiffer` trait | `change_facts.rs:64` | `fn diff(&self, repository_root: &str, rev_base: &str, rev_head: &str)` — the whole trait is stringly-typed |
| `listing_at` | `change_facts.rs:74,77` | `revision: &str` → `Revision::Named(Arc::from(revision))` |
| `GitRevisionExecutor::pair` | `hosts.rs:549-550, 578-579` | `rev_a: &str, rev_b: &str` → two `Revision::Named` |
| `CheckoutTrees::read_each` | `dep_resolve.rs:469,476` | `revision: &str` → `Revision::Commit(ObjectId(revision.into()))` |
| `CheckoutTrees::head` | `dep_resolve.rs:454-461` | `Revision::Named("HEAD")` resolved, then immediately flattened back to `String` via `oid.0.to_string()` |
| `revision_oid` | `source_bind/_1_runtime.rs:329-343` | `RevisionId` → `format!("worktree:{}:{}:{}", worktree.0, head, dirty)` — a **hand-rolled string encoding of `RevisionId::Worktree`** that soopy already serializes structurally (`_0_types.rs:251` derives `Serialize`) |
| memo keys | `hosts.rs:244, 556, 721` | `format!("{frontier}\|{checkout_root}\|{seed}")`, `format!("{repo}\|{rev_a}\|{rev_b}")` — string-concatenated cache keys over untyped revs |

## 3.4 Path / pattern types

| sprefa side | soopy side |
|---|---|
| `sprefa-extract/src/types.rs:32` `pub struct Span { start, len: u32 }` | `soopy/src/_0_types.rs:322` `SourceSpan { source: SourceRef, start, end: u64 }` — soopy's span carries its own coordinate; extract's is bare and file-local |
| Host input `glob: text` as a raw `String` (`hosts.rs:133-136`), fed to `GitFilesQuery.pathspecs: Vec<String>` | `soopy::Pattern` (`_1_pattern.rs:6`) exists but is **never used by the engine** (0 hits in `src`) |
| `sprefa-extract/src/types.rs:878` `pub struct FileSet;` (a unit stub, used at `project.rs:142`) | `soopy::FileSnapshot`/`FileEntry` (`_0_types.rs:117,131`) — unused by extract |
| `ReadRequestWire` (`sprefa-v6/0_runtime/1_rust_runtime_host/src/_0_types.rs:24`, with `From` both ways at `:29,:38`) | `soopy::ReadRequest` (`_0_types.rs:291`) — a pure serde twin; the comment at `:21-22` states the reason (soopy's `ReadRequest` has no serde impl) |
| paths as bare `String` everywhere in hosts (`hosts.rs:176-181`, `change_facts.rs:39,45,50`) | `soopy::RepoPath` (`_0_types.rs:224`) — **0 uses in either consumer** |

`path_from_cwd` (`hosts.rs:188-212`) hand-rolls relative-path rendering that duplicates soopy's own cwd-relative pathspec logic at `_9_git_files.rs:43-67` (`pathspec_at`).

---

# 4. DL6 HOSTING SEAM

## 4.1 Dispatch table

`executor_for_plan` (`hosts.rs:56-78`) is the sole router; `executor_for` (`hosts.rs:43-50`) is its fallback. An unrouted plan is a hard error at construction (`hosts.rs:1267-1272`).

| # | Host family | Names | Const array | Executor | Cite | Path to bytes |
|---|---|---|---|---|---|---|
| 1 | Git file feeds | `files`, `files_at`, `repo_files`, `repo_files_at` | *inline `matches!`*, `hosts.rs:58-61` (no const array) | `SoopyFilesExecutor` | `hosts.rs:120-186` | **soopy** — `discover`(:157), `SourceTree::open`(:158), `git_files_from`(:160) |
| 2 | Dependency crawl | `dep_crawl_repo/_visited/_edge/_unresolved` | `DEP_CRAWL_HOSTS`, `hosts.rs:218-223` | `DepCrawlExecutor` (static `DEP_CRAWL`, :225) | `hosts.rs:227-382` | **mixed** — soopy for git (`dep_resolve.rs:436,442,454,478,490`), **raw `std::fs`** for manifests (`dep_resolve.rs:150` `read_dir`, `:410,:418` `read_to_string`) |
| 3 | Refs / tags | `git_ref`, `git_tag` | `GIT_REF_HOSTS`, `hosts.rs:388` | `GitRefExecutor` (static `GIT_REFS`, :402) | `hosts.rs:405-537` | **soopy** — `discover`(:443), `RefQuery`(:445), `Refs::open(..).snapshot()`(:452-453) |
| 4 | Revision graph | `git_merge_base`, `git_ahead_behind`, `git_ancestor` | `GIT_REVISION_HOSTS`, `hosts.rs:392` | `GitRevisionExecutor` (static `GIT_REVISIONS`, :403) | `hosts.rs:539-697` | **soopy** — `discover`(:560), `RevisionGraph::open`(:563), two batched `graph.query`(:576, :612) |
| 5 | Rev-pair change plane | `git_change`, `git_rename`, `git_changed_line` | `GIT_CHANGE_HOSTS`, `hosts.rs:703` | `ChangeFactExecutor` (static `GIT_CHANGES`, :705) | `hosts.rs:707-805` → `change_facts.rs:151-222` | **soopy** — `discover`(:153), `git_files`(:76), `GitBatch::open`(:193), `batch.read`(:199,:204) |
| 6 | In-process extraction | any plan with `execution == "sprefa_extract"` or `"sprefa_extract_repo"` | *no array* — `hosts.rs:47` | `SprefaExtractExecutor` | `hosts.rs:115`, impl `hosts.rs:807-892` | **RAW `std::fs`** — see 4.2 |
| 7 | Everything else | any `execution == "shell"` not matched above | — | `ShellExecutor` | `hosts.rs:86-113` | **RAW `std::process::Command::new("sh")`** (`hosts.rs:95`) |

**scip hosts: none exist in the engine.** The only `scip` token in `sprefa-engine-rs/src` is the refusal arm at `hosts.rs:864-869`: `"scip" | "diet_scip" => return Err(named(format!("mode `{}` is not linked in-process", …)))`. SCIP is reachable only by falling through to `ShellExecutor` spawning the `extract` binary, which then runs `scip_ensure.rs:435` `Command::new(program)` and `scip_decode.rs:27,51` `std::fs::read`.

## 4.2 SprefaExtractExecutor reads via `std::fs::read` — CONFIRMED

`/Users/chrishafley/projects/sprefa/v6/sprefa-engine-rs/src/hosts.rs:852-853`:

```rust
let content = std::fs::read(&path)
    .map_err(|failure| named(format!("read {path} failed: {failure}")))?;
```

`path` is a token scraped out of the *filled shell template* by `shell_tokens` (`hosts.rs:818`, tokenizer at `:896-950`), selected as "the first non-`--` token" at `hosts.rs:848`. It is a worktree path. There is **no soopy call, no `SourceRef`, no `ContentId`** anywhere in `hosts.rs:807-892` — `grep -c "soopy" hosts.rs` puts all 25 hits outside this range.

The bytes then go to `sprefa_extract::file_fact(&path, &content)` (`hosts.rs:882`) and `sprefa_extract::dispatch(&path, &content, mask)` (`hosts.rs:885`).

## 4.3 Memoisation table

| Executor | Memo field | Key | Cite |
|---|---|---|---|
| `DepCrawlExecutor` | `crawls: Mutex<BTreeMap<String, Arc<DepResolveOutcome>>>` (`:229`) | `format!("{frontier}\|{checkout_root}\|{seed}")` | `hosts.rs:244-247, 265-268` |
| `GitRefExecutor` | `snapshots: Mutex<BTreeMap<String, Arc<RefSnapshot>>>` (`:407`) | **`repo` alone** — no rev, no ref-name, no mtime | `hosts.rs:440, 456-459` |
| `GitRevisionExecutor` | `pairs: Mutex<BTreeMap<String, Arc<RevisionGraphResult>>>` (`:541`) | `format!("{repo}\|{rev_a}\|{rev_b}")` (unresolved spellings, so `HEAD` memoises) | `hosts.rs:556-558, 624-627` |
| `ChangeFactExecutor` | `diffs: Mutex<BTreeMap<String, Arc<RevisionDiff>>>` (`:709`) | `format!("{repo}\|{rev_base}\|{rev_head}")` | `hosts.rs:721-723, 733-736` |
| `SoopyFilesExecutor` | **none** | — | `hosts.rs:122-186` is stateless; re-runs `ls-files` + `hash-object` per invocation |
| `SprefaExtractExecutor` | **none of its own**; folded by the runner | applicative group key `format!("{execution}\|{template}\|{ordered_inputs:?}")` where `ordered_inputs` = every declared plan input incl. `digest` | `hosts.rs:1407-1425`; gated by `is_applicative` (`:82-84`, `:1403`) |
| all plans (dedupe layer) | `claimed: HashSet<String>` | `format!("{plan_name}\|{witness_digest}")` | `hosts.rs:1329-1331, 1391-1398` |

`GitRefExecutor`'s repo-only key (`hosts.rs:440`) is the loosest: refs move, the memo does not.

## 4.4 The rev-pin identity defect, precisely

The dl6 declaration for the repo-scoped extract host (`v6/tsv2/gen_served/44a6494405222cdc9132718fe4d7e7ae.dl6:116-117`):

```
sh repo_extract(repo: text, path: text, digest: text) -> (callee: text) =
  `"$DL_EXTRACT_BIN" --family call {repo}/{path}`.
```

and its own comment at `:113-115`: *"`digest` is freshness — **the template never mentions it** — so re-crawling a repository whose blob oids have not moved is a cache hit per file"*. The unscoped twin is identical (`gen_served/4e85d7280bc6cc29358e6f83e000090b.ts:60`: `call_node` / `call_ref`, inputs `path`+`digest`, template `"$DL_EXTRACT_BIN" --family call {path}`). Executor selection is by template shape (`prolog/compile/registry.pl:369-380`).

`digest` comes from `repo_files_at` (`.dl6:99-104`), which under `Revision::Named` resolves to a **commit** and produces the blob OID at that commit (`soopy/src/_9_git_files.rs:38 → commit_rows :142-188`). So:

- the **memo key** is rev-correct (digest is inside `ordered_inputs`, `hosts.rs:1407-1421`),
- the **bytes are not** (`hosts.rs:852` reads the worktree file at `{repo}/{path}`),
- the extract binary has **no rev/digest flag at all**: `grep -n "rev\|revision\|digest" src/bin/extract.rs` returns one hit, a doc comment at `:122`. Its only read is `std::fs::read(path)` at `bin/extract.rs:313`.

Net: a cache hit on a stale digest returns facts extracted from *current disk*; a cache miss on a moved digest also returns facts from current disk. The digest never gates content, only identity.

---

# 5. THE WORKTREE QUESTION

## 5.1 Layer-by-layer

| Layer | Cite | Behaviour |
|---|---|---|
| soopy CLI | `main.rs:125-131` | `"WORK"` is the sentinel string → `Revision::Worktree`; anything else → `Revision::Named` |
| soopy resolve | `_3_revision.rs:8-23` | `Worktree` → `RevisionId::Worktree { worktree, head: rev_parse("HEAD").ok(), dirty: dirty()? }`; `dirty` from `git status --porcelain -z --untracked-files=normal` (`:41-55`) — a failed `git status` is a **hard error**, never inferred clean (`:48-53`) |
| soopy fs-glob enumeration | `_7_source_tree.rs:44-49` → `_4_worktree.rs:27-108` | walks disk with `ignore::WalkBuilder`, hashes bytes → `ContentId::Blake3` (`:81`). **Sees untracked + dirty.** |
| soopy tracked enumeration | `_9_git_files.rs:36-40` | `Worktree` → `worktree_rows` (`:105-140`): `git ls-files` **without** `--with-tree` (`:79-82`), then `git hash-object --stdin-paths` over the **on-disk** files (`:113`) → `ContentId::GitBlob`. **Sees dirty content on tracked paths; does NOT see untracked paths.** |
| soopy read | `_7_source_tree.rs:150-172` | `RevisionId::Worktree` → `std::fs::File::open(root.join(path))` (`:157`) — always current disk; digest variant honours the caller's `expected` (`:165-170`) |
| engine files hosts | `hosts.rs:147-155` | `files`/`repo_files` → `Revision::Worktree`; `files_at`/`repo_files_at` → `Revision::Named(env["rev"])` |
| engine change facts | `change_facts.rs:74-79` | `listing_at` hard-codes `Revision::Named(Arc::from(revision))` — **`Revision::Worktree` is unreachable** from this path |
| engine dep crawl | `dep_resolve.rs:454-461` | resolves `Revision::Named("HEAD")` and **bails** if it comes back `RevisionId::Worktree` (`:457-460`); reads use `Revision::Commit` (`:476`) |
| engine source_bind | `_0a_inputs.rs:82-98` | routes `RevisionId::Worktree` reads to the owning worktree's `SourceTree::read_each` — worktree-aware; `_1_runtime.rs:332-341` encodes `worktree:{id}:{head}:{dirty}` |
| runtime host | `sprefa-v6/.../1_rust_runtime_host/src/_1_source_host.rs:201,243` | issues `Revision::Worktree` for the untagged file demands |

## 5.2 Verdict per host

| Host | Sees dirty worktree? | Sees untracked files? | Cite |
|---|---|---|---|
| `files`, `repo_files` | **YES** (hash-object over disk) | **NO** (`ls-files` with no `--with-tree`, index-only path list) | `hosts.rs:148` → `_9_git_files.rs:37,79-82,113` |
| `files_at`, `repo_files_at` | **NO** — commit-pinned | NO | `hosts.rs:149` → `_9_git_files.rs:38,80` |
| `git_change`, `git_rename`, `git_changed_line` | **NO — structurally impossible** | NO | `change_facts.rs:77` is `Revision::Named` only |
| `git_ref`, `git_tag` | N/A (refs, not files); memo never invalidates | — | `hosts.rs:440` |
| `git_merge_base`, `git_ahead_behind`, `git_ancestor` | **NO** — `Revision::Named` both sides | — | `hosts.rs:578-579` |
| `dep_crawl_*` (git leg) | **NO** — bails on worktree (`:457`) | — | `dep_resolve.rs:454-461, 476` |
| `dep_crawl_*` (manifest leg) | **YES — unconditionally, no rev at all** | YES | `dep_resolve.rs:150, 410, 418` (raw `std::fs`) |
| `sprefa_extract` / `sprefa_extract_repo` | **YES — unconditionally, regardless of the `digest` input** | YES | `hosts.rs:852` |
| generic `shell` | whatever the template does | — | `hosts.rs:95` |
| source_bind identity path | **YES, correctly** — worktree identity carried through `RevisionId::Worktree` | per query | `_0a_inputs.rs:82`, `_1_runtime.rs:332` |

The asymmetry the fixture exercises: `tests/live_hosts.rs:235-243` writes `VERSION = 1`, commits, overwrites with `VERSION = 2`, and adds `src/untracked.rs`. `repo_files` returns the `= 2` hash-object OID; `repo_files_at HEAD` returns the `= 1` commit OID; neither returns `src/untracked.rs`.

---

# 6. EXTRACT'S INTERNAL READ PATHS

## 6.1 Full inventory — `sprefa-extract/src`

Measured: `std::fs::read(` = **6**, `std::fs::read_to_string(` = **3**, `read_dir` = 1, `Command::new` = 3.

| # | Site | Call | Through `BlobSource`? | Rev-aware? |
|---|---|---|---|---|
| 1 | `project.rs:691` | `std::fs::read(self.root.join(path))` | **IS** the `FsBlobSource` impl | **NO** — plain directory, no revision |
| 2 | `project.rs:672` | `tree.read_many(&[request])` | **IS** the `SourceTreeBlobSource` impl | **YES** — `expected: Some(entry.content)` at `:668` verifies content | 
| 3 | `project.rs:379` | `std::fs::read(path)` in `read_inputs` | **BYPASS** | **NO** — the corpus ingest for `resolve_project`/`scip_facts`/`diet_scip`; hashes to `BlobHash::of(&content)` at `:383` |
| 4 | `bin/extract.rs:313` | `std::fs::read(path)` | **BYPASS** | **NO** — the CLI's single-file read; the only read the `extract` binary performs |
| 5 | `0_query.rs:55` | `std::fs::read(path)` | **BYPASS** | **partially** — `source_bytes(path, digest)` at `:52-57`: if `digest` is `Some`, calls `cat_blob` instead |
| 6 | `0_query.rs:60-66` | `Command::new("git") cat-file blob {oid}` | **BYPASS** | **YES, but re-implements soopy** — this is a hand-rolled one-shot of `soopy::GitBatch::read` (`_6_git_batch.rs`) |
| 7 | `scip_decode.rs:27` | `std::fs::read(index_path)` | BYPASS | N/A (index artifact) |
| 8 | `scip_decode.rs:51` | `std::fs::read(path)` | BYPASS | N/A |
| 9 | `deps.rs:139` | `std::fs::read_to_string(root.join("tsconfig.json"))` | **BYPASS** | **NO** — manifest read off the worktree |
| 10 | `scip_ensure.rs:327` | `std::fs::read_to_string(&gitignore)` | BYPASS | NO |
| 11 | `scip_ensure.rs:498` | `std::fs::read_to_string(path)` | BYPASS | NO |
| 12 | `scip.rs:394` | `std::fs::read_dir(&dir)` | BYPASS | NO |
| 13 | `scip_ensure.rs:435`, `:486` | `Command::new(program)` / `Command::new("kill")` | BYPASS | N/A (indexer subprocess) |

**Score: 2 of 13 read paths go through `BlobSource`, and only 1 of those (`SourceTreeBlobSource`, #2) is revision-pinned — and it has zero production callers (§3.2).** Every read that actually runs in production (#1, #3, #4, #5-without-digest, #9) is raw worktree disk.

## 6.2 The clean counter-example inside the engine

`sprefa-engine-rs/src/source_bind/_1_runtime.rs:207-234` (`extract_many`) is the shape everything else should have:

```rust
self.inputs.read_each(&requests, &mut buffer, |result| {
    extracted.insert(result.source.clone(),
        extract_specifiers(&relations, result.source, result.content, result.bytes, &roots));
    Ok(())
})?;
```

soopy owns the read and the revision; `extract_specifiers` (`:382-395`) calls `sprefa_extract::dispatch(source.path.0.as_ref(), bytes, FamilyMask::ALL)` — **path for language selection, bytes from soopy**. `dispatch`'s signature (`sprefa-extract/src/dispatch.rs:14` `pub fn dispatch(path: &str, content: &[u8], mask: FamilyMask)`) already accepts caller-supplied bytes, so nothing in extract's core needs changing to make every caller rev-correct.

---

# COLLAPSE CANDIDATES

Ordered by how much identity they currently break.

| # | Duplicate / bypass | Cite | soopy (or existing) call that replaces it |
|---|---|---|---|
| 1 | `SprefaExtractExecutor` reads worktree disk while the plan carries a rev-pinned `digest` | `hosts.rs:852` | Build `soopy::ReadRequest { source, expected: Some(ContentId::GitBlob(digest)) }` from the demand row and call `SourceTree::read_each`, then `sprefa_extract::dispatch(path, bytes, mask)` — the exact shape already working at `source_bind/_1_runtime.rs:220-232`. Requires the host plan to also carry `repo` + `rev`, which `repo_extract` (`.dl6:116`) already declares as `repo`. |
| 2 | `FsBlobSource` (raw `std::fs::read`) is the only production `BlobSource`; the rev-correct twin is dead | `project.rs:691` vs `project.rs:663-673`; call sites `project.rs:147, 195` | Swap `FsBlobSource::new(root)` for `SourceTreeBlobSource::open(root, revision, patterns)` — same trait, already tested at `tests/10_source_tree.rs:42-57`. |
| 3 | `read_inputs` corpus ingest bypasses `BlobSource` entirely | `project.rs:376-390` (`std::fs::read` at `:379`) | Take `&dyn BlobSource` (or a `SourceTreeBlobSource`) as a parameter; `soopy::SourceTree::read_each` for the whole corpus in one batch instead of a per-path `fs::read` loop. |
| 4 | `BlobHash` (truncated blake3-16) as the corpus content key, incomparable with `ContentId` | `types.rs:55-64`; 82 refs | `soopy::ContentId` (`_0_types.rs:266`) — it already covers both `GitBlob` and full `Blake3`, so a digest from `repo_files_at` and a digest from extract become the *same value*. Also drops extract's separate `blake3 = "1"` dep (`Cargo.toml:89`). |
| 5 | `0_query.rs` hand-rolls `git cat-file blob` | `0_query.rs:60-90` | `soopy::GitBatch::open(root)` + `.read(&ObjectId(...))` — the batched form already used at `change_facts.rs:193-205`. One long-lived process instead of one spawn per blob. |
| 6 | `revision_oid` hand-encodes `RevisionId::Worktree` into `"worktree:{id}:{head}:{dirty}"` | `source_bind/_1_runtime.rs:329-343` | `serde_json::to_string(&revision)` — `RevisionId` derives `Serialize` at `_0_types.rs:251` and the doc at `:69` says serialization is structural, never a display string. |
| 7 | `path_from_cwd` re-derives cwd-relative repo paths | `hosts.rs:188-212` | `soopy::_9_git_files::pathspec_at` logic (`_9_git_files.rs:43-67`), already reached by passing `cwd` to `git_files_from` — which `hosts.rs:165` already does; the post-hoc rewrite at `:177` is then redundant. |
| 8 | `IRevisionDiffer` + `listing_at` are stringly-typed and can never name the worktree | `change_facts.rs:64, 74-79` | Take `soopy::Revision` (`_0_types.rs:239`) instead of `&str`; `Revision::Worktree` then flows to `_9_git_files.rs:37` and `git_change` gains dirty-worktree diffs for free. |
| 9 | dep-crawl manifest leg reads raw worktree disk while its git leg is rev-pinned | `dep_resolve.rs:150, 410, 418` vs `:476` | Route `go.mod`/`package.json` through the same `CheckoutTrees::read_each` (`dep_resolve.rs:466-494`) already used for the source files — one `Revision::Commit` for the whole crawl. |
| 10 | `GitRefExecutor` memo keyed on `repo` only | `hosts.rs:440, 456-459` | Add a `soopy::RefSnapshot` freshness witness to the key, or subscribe via `soopy::RepositoryWatcher` (`_8_watch.rs:120`) — currently 100% dead surface. |
| 11 | Host `glob` inputs are bare `String` | `hosts.rs:133-136`, `GitFilesQuery.pathspecs` | `soopy::Pattern` (`_1_pattern.rs:6`) for the fs-glob hosts; keep raw `String` only where Git pathspec semantics are genuinely wanted (`_0_types.rs:385-391` documents the distinction). |
| 12 | `ReadRequestWire` serde twin | `sprefa-v6/0_runtime/1_rust_runtime_host/src/_0_types.rs:24-45` | Add `#[derive(Serialize, Deserialize)]` to `soopy::ReadRequest` (`_0_types.rs:290-294`) — every field is already serde-capable, and the twin's own comment (`:21-22`) names this as the only reason it exists. |
| 13 | Two lockfiles resolve soopy's `ignore`/`blake3`/`globset` differently | `sprefa-extract/Cargo.lock` vs `sprefa-engine-rs/Cargo.lock` | Drop the standalone `[workspace]` in one of the two (`sprefa-extract/Cargo.toml:4` / `sprefa-engine-rs/Cargo.toml:1`), or add matching `[patch]`/`=`-pins, so both builds of soopy walk the worktree with the same `ignore`. |
