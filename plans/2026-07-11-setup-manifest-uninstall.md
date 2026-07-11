# Setup manifest + clean uninstall ("i don't like it, remove it")

Chris ask (2026-07-11): every `dl setup` wiring must be reversible. Today
setup writes symlinks, JSON merges, plugin files, and CLAUDE.md/AGENTS.md
appends across three harnesses with NO record — uninstall means archaeology.

## Design: record-at-write, reverse-from-record

### Storage
`$XDG_STATE_HOME/sprefa/setup-manifest.json` — one global file, array of
entries; per-root entries carry the root path. Append-only journal +
compaction on read (an entry superseded by a re-run replaces its precursor,
keyed on (root, target-path, kind)). NOT in the repo (setup touches files
outside repos too: ~/.agents/skills, ~/.codex, global CLAUDE.md).

### Entry shape (type signatures)
```rust
struct SetupEntry {
  root: Option<PathBuf>,      // None = global wiring
  target: PathBuf,            // the file we touched
  kind: SetupKind,
  detail: SetupDetail,        // enough to REVERSE, not a snapshot
  wrote_at: i64,
  dl_version: String,
}
enum SetupKind { Symlink, FileCreate, JsonMerge, MarkedAppend, HookRegister }
enum SetupDetail {
  Symlink { points_to: PathBuf },
  FileCreate { content_blake3: String },        // delete only if unmodified
  JsonMerge { pointer: String, added: serde_json::Value }, // exact node we added
  MarkedAppend { begin_marker: String, end_marker: String },
  HookRegister { event: String, command_substring: String },
}
```

### Reversal rules (the correctness core)
- Symlink: remove only if it still points where we pointed it.
- FileCreate: delete only if blake3 matches what we wrote; else SKIP LOUDLY
  ("modified since install, left in place: <path>").
- JsonMerge: remove exactly the node we added (hooks arrays: match by
  command substring, the register_hook_event dedup key); leave the rest of
  the user's file byte-stable; delete the file only if OUR removal leaves
  the pre-existing-empty shape AND we created it.
- MarkedAppend (CLAUDE.md/AGENTS.md sections): strip between our markers
  only; existing setup code must start writing markers where it doesn't.
- Never touch anything absent from the manifest. Never rm -rf a directory.

### CLI surface
- `dl setup --list` — render the manifest (what's wired, where, when).
- `dl setup --undo [--root <r>] [--global] [--dry-run]` — reverse; dry-run
  prints the exact actions; every skip is loud with the reason.
- `dl uninstall` = `setup --undo` everything + print the two things dl can't
  do itself (cargo uninstall dl; codex trust entry removal is manual).
- Emergency-stop (hooks off, daemon stop) documents alongside; --undo is the
  polite twin of the panic runbook.

### Migration for existing installs
Manifest starts empty on machines wired before this lands. `dl setup
--adopt` re-detects known shapes (our symlink targets, our hook command
substrings, our markers) and backfills entries it is CONFIDENT about;
everything else stays unlisted and untouched. Re-running `dl setup` after
upgrade also backfills naturally (idempotent writes re-record).

### Lifetimes / writes
Manifest loaded+compacted at setup/undo entry, written once per command
(atomic tmp+rename). Setup fns get a `&mut SetupJournal` threaded through —
each wire_* records at its own write site (turnkey: forgetting to record is
impossible if the journal owns the write helpers, SymSink precedent).

## Staffing
terra, one worktree, after the README arc lands (setup.rs/hooks.rs just
split — build on the new layout). Tests: wire a scratch repo across all
three harness shapes -> --undo -> tree byte-identical to pre-setup; modified
-file skip; --adopt backfill; dry-run exactness.
<!-- todo(feature): setup manifest + dl setup --undo/--list/--adopt + dl uninstall -->
<!-- todo(docs): README emergency-stop section links setup --undo as the polite twin -->

## Paranoia invariants (Chris directive: never hurt a user's existing setup)

WRITE-SIDE (setup), enforced in the journal-owned helpers so no wire_* can
bypass them:
1. NEVER overwrite. A path that already exists and is not byte/target-
   identical to what we would write = loud skip ("exists, not ours, left
   alone"), NOT a backup-and-replace. This covers a user's own skill named
   like ours, their own hooks.json entries, their own plugin file.
2. Symlinks: create only when the path is ABSENT. If a real file/dir sits at
   .claude/skills/<name>, we never replace it with a link. If a symlink
   exists pointing elsewhere (their own link), skip loudly.
3. JSON merge: parse-preserve everything (existing entries never reordered,
   rewritten, or deduped beyond our own command-substring key); if the file
   fails to parse, SKIP — never "fix" a user's malformed settings.
4. Marked appends: if our begin marker exists without the end marker (user
   edited inside), skip and say so; never re-append a second copy.
5. Path hygiene: canonicalize and verify every target stays under the
   expected root (~/.claude, <repo>/.claude, ~/.agents, <repo>/.codex,
   <repo>/.opencode); refuse to write through a symlinked PARENT directory
   that escapes those roots (symlinked-dotdir exfil/clobber class).
6. Atomic writes only (tmp + rename, same filesystem); no partial states.
7. No recursive operations, ever: no rm -rf, no directory copies; every
   action names ONE path.

UNDO-SIDE additions (beyond the reversal rules above):
8. Undo re-verifies the paranoia checks at removal time (a path that
   canonicalizes outside the expected roots is skipped even if the manifest
   claims it — a moved/replaced dotdir must not redirect our delete).
9. --dry-run output is the exact action list; tests assert dry-run and real
   run touch identical path sets.

TESTS (red side is the point): pre-existing user skill with our name
survives setup byte-identical; user hooks entries survive our merge and our
undo; malformed settings.json = skip not crash; symlinked .claude dir
pointing at $HOME refuses; marker-tampered CLAUDE.md skips; dry-run/real
parity.
<!-- todo(feature): paranoia invariants in journal-owned setup helpers + red-side tests -->
