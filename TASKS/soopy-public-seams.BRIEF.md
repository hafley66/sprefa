# soopy-public-seams

Repo: `~/projects/hafley-rs`. Crate: `crates/soopy`.
Two mechanical public-surface additions. No behavior change anywhere.

## First action

```bash
git merge --ff-only 57c8225e6ced64833da1a8569b9701a05b8be418
```

Failure = STOP AND REPORT. Do not work around it.

## Ownership

You own ONLY:
- `crates/soopy/src/_0_types.rs`
- `crates/soopy/src/_4_worktree.rs`
- `crates/soopy/src/_3a_files.rs`
- `crates/soopy/src/_7_source_tree.rs`
- `crates/soopy/src/_7d_mutation_plan.rs`
- `crates/soopy/src/_7f_commit.rs`
- `crates/soopy/src/_8_watch.rs` (ONE visibility change, Task 3)
- `crates/soopy/src/lib.rs` (ONE re-export line, Task 3)
- `crates/soopy/tests/2_identities.rs`

FORBIDDEN, do not open, do not edit: `crates/boop/**`, `crates/boop-mux/**`,
every other crate in the workspace, every other file under `crates/soopy/`
(including `main.rs`, `_11_refs.rs`), `issues/**`, `chat_log/**`, `plans/**`.

## Task 1: a public `ContentId` blake3 constructor

`ContentId` is declared at `crates/soopy/src/_0_types.rs:266-269`:

```rust
pub enum ContentId {
    GitBlob(ObjectId),
    Blake3([u8; 32]),
}
```

The variant is public but every producer hand-writes the hashing expression.
Six sites do it:

| file:line | current expression |
|---|---|
| `_4_worktree.rs:81` | `ContentId::Blake3(*blake3::hash(&bytes).as_bytes())` |
| `_3a_files.rs:75` | `ContentId::Blake3(*blake3::hash(&bytes).as_bytes())` |
| `_3a_files.rs:128` | `ContentId::Blake3(*blake3::hash(&buffer).as_bytes())` |
| `_7_source_tree.rs:169` | `ContentId::Blake3(*blake3::hash(buffer).as_bytes())` |
| `_7d_mutation_plan.rs:888-890` | private `fn blake3_content(bytes: &[u8]) -> ContentId` |
| `_7f_commit.rs:861` | `ContentId::Blake3(*blake3::hash(&bytes).as_bytes())` |

Add, in `_0_types.rs`, next to the `ContentId` declaration:

```rust
impl ContentId {
    /// Hash bytes to the worktree content identity. The one place the blake3
    /// expression lives, so a caller outside this crate never re-derives it.
    pub fn blake3(bytes: &[u8]) -> Self {
        Self::Blake3(*blake3::hash(bytes).as_bytes())
    }
}
```

Then replace all six sites above with `ContentId::blake3(&bytes)` /
`ContentId::blake3(buffer)` as the local binding requires. DELETE the private
`blake3_content` helper in `_7d_mutation_plan.rs:888-890` and update its two
callers at `_7d_mutation_plan.rs:118` and `:139` to `ContentId::blake3(&bytes)`
and `ContentId::blake3(&bytes_after)`.

`_7f_commit.rs:861` sits inside a `match` arm
(`ContentId::Blake3(_) => ...`); only the right-hand expression changes.

`ContentId` is re-exported by `pub use _0_types::*;` at
`crates/soopy/src/lib.rs:28`, so the new method needs no export edit. Do NOT
edit lib.rs.

## Task 2: serde derives on `ReadRequest`

`ReadRequest` is at `_0_types.rs:291-294`:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadRequest {
    pub source: SourceRef,
    pub expected: Option<ContentId>,
}
```

Both field types already derive `Serialize, Deserialize` (`SourceRef` at
`_0_types.rs:275-280`, `ContentId` at `:265-269`). Add `Serialize, Deserialize`
to `ReadRequest`'s derive list. Nothing else on this type changes.

Do the SAME for `FileReadRequest` (`_0_types.rs:137-141`) only if it compiles
with no further change; if any field type lacks the derives, leave
`FileReadRequest` alone and say so in the PR body. Do not add derives to any
other type.

## Task 3: publish `git_dirs`

`crates/soopy/src/_8_watch.rs:579` declares:

```rust
fn git_dirs(root: &Path) -> Result<(PathBuf, PathBuf)> {
```

returning `(git_dir, common_dir)`. It is private, so a consumer outside this
crate re-derives the Git directory by hand. Make it `pub`, give it a doc
comment naming what the two returned paths are and that the ref store lives in
the common dir, and add ONE line to `crates/soopy/src/lib.rs` next to the
existing `pub use _8_watch::{DirectoryWatcher, RepositoryWatcher, SourceWatcher};`
at `lib.rs:47`, widening that list to include `git_dirs`.

Change NOTHING else in `_8_watch.rs` and NOTHING else in `lib.rs`. The function
body stays byte-identical.

## Test to add

In `crates/soopy/tests/2_identities.rs`, append two tests:

1. `content_id_blake3_constructor_matches_variant`: assert
   `ContentId::blake3(b"hello") == ContentId::Blake3(*blake3::hash(b"hello").as_bytes())`.
   If `blake3` is not already a dev-dependency or dependency visible to the
   test, assert instead that `ContentId::blake3(b"hello").to_string()` starts
   with `"blake3:"` and that two calls on equal bytes are equal while calls on
   different bytes differ. Pick whichever compiles; do not add a dependency.
2. `read_request_round_trips_through_json`: build a `ReadRequest`, run it
   through `serde_json::to_string` then `serde_json::from_str`, assert equality.
   Follow the construction shape already used elsewhere in that test file for
   `SourceRef`; read the file before writing.

## Validation, run it exactly

```bash
cd ~/projects/hafley-rs && cargo test -p soopy 2>&1 | tail -30
cd ~/projects/hafley-rs && cargo build 2>&1 | tail -20
```

Both must be rc=0. Run `cargo test -p soopy` TWICE and put both pass/fail
counts in the PR body. Also paste the output of:

```bash
cd ~/projects/hafley-rs && grep -rn "ContentId::Blake3(\*blake3" crates/soopy/src/
```

which must be EMPTY after the change except for nothing at all (the constructor
body itself uses `Self::Blake3(*blake3::hash(...))`, which this grep does not
match).

## Style laws

- `tracing` only; no `eprintln!` in `src/**`.
- Comment budget: a comment states only a constraint the code cannot show. No
  change-log narrative, no dates, no arc references, no restating the next line.
- BANNED words in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source, base, critical, mode.
- The word "refusal" is banned in prose.
- No em dashes. Descriptive names, never single letters.
- Colocated consistency: match the surrounding file's existing style.

## Landing

Branch is already checked out for you. Commit with trailer
`Refs-Issue: @soopy-typed-seams`, push, and open the PR:

```bash
gh pr create --title "soopy: public ContentId::blake3 constructor + ReadRequest serde" --body "<receipts>"
```

DO NOT merge. DO NOT push to main. You never spawn subagents.
