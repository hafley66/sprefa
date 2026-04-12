# `path()` Tag — Virtual FS for Rule Branching

## Context

Block semantics in NOTES.md (Feature 5) need a disambiguator for branching inside `rule` bodies. The language already has:

- `A > B` — chain (AND, one rule, same row)
- `{ A; B }` — fork (OR via monomorphization, `_3_lower.rs:280`)
- Block-with-keys alternation in `json({ key: ...; key: ... })` where keys drive pattern branches

Adding a dedicated flow-control keyword or `label:` prefix syntax duplicates mechanisms. Reuse what's there.

## Design

### `path()` tag — two exclusive forms

**Form A — `path(PATTERN)`, 1 arg, no block: filter mode.**
```
> path(/rust/**)     # glob
> path(re:^/ts/.*)   # regex
```
Reuses the pattern grammar `fs()` accepts. Filters cursors whose `$_path` matches.

**Form B — `path() { k: ...; k: ... }`, no arg, with block: branching mode.**
```
rule classify {
  fs(**/*) > path() {
    rust: fs(**/*.rs);
    ts:   fs(**/*.ts);
  }
}
```
Each block key appends a segment to the cursor's virtual path. Same parser mechanism as `json({ key: ...; key: ... })`.

These forms are exclusive — `path(PATTERN) { ... }` is a parse error.

### Virtual FS framing

The accumulated path through nested `path() {}` blocks is shaped like a filesystem path (`/`-separated). Any tag that already matches path-shaped data (`fs`, `file`, `folder`, `path`) works on it. Root is `/`. Each `path()` block descends a level. Nested:

```
path() {
  rust: path() {
    test: re:#\[test\];
    prod: re:pub\s+fn;
  };
  ts: fs(**/*.ts);
}
```
→ virtual paths `/rust/test`, `/rust/prod`, `/ts`.

### Storage

`$_path` synthesized capture on each monomorphized rule, populated at lowering time from the key chain. Same-name rules already union (README:128), so branches land in one `{name}_data` table with a `_path` column. Downstream `check` filters with SQL:

```
check classify_rust_only { SELECT * FROM classify_data WHERE _path LIKE '/rust/%' }
```

No new `match` tag — `check`'s SQL is the dispatch layer, `path()` downstream in the rule body is the filter layer. Pick whichever fits.

### AND vs OR revisited

- `A > B`         — AND (same row, sequential narrowing)
- `path() {;}`    — OR, keyed, path-tracked (Form B)
- `path(PAT)`     — filter on accumulated path (Form A)
- `{ A; B }`      — OR, unkeyed (existing mechanism, `_path` stays NULL)

`mergeByKey` from the rxjs analogy = SQL union on `_path`. Existing lowering already does the merge; `path()` adds the key.

### Homoiconicity angle

`path()` accepting the same pattern grammar as `fs()` means rules that produce path-shaped data become queryable by path-shaped tags. The virtual eval tree and the real filesystem share an interface. Rules-as-expressions (rule inline where a tag can appear) is the longer-term follow-on; `path()` is the first piece.

## Files to Modify

- `crates/sprf/src/_0_ast.rs` — add `Tag::Path` variant (both block and pattern forms)
- `crates/sprf/src/_1_parse.rs` — parse `path()` with existing block-key + pattern-arg mechanisms (no new grammar)
- `crates/sprf/src/_3_lower.rs` — during fork expansion (~line 280), when a `path()` block is expanded, thread the key chain; synthesize `$_path` MatchDef on each monomorph (~line 270); when `path(PATTERN)` is encountered, lower to a filter on the `_path` column
- `crates/rules/src/types.rs` — `SelectStep` variant for path filtering, or reuse file-pattern step with a "source: virtual_path" flag
- `crates/sprf/tests/fixtures/kitchen_sink.sprf` — labeled `path()` rule + downstream `path(...)` filter + `check` filtering on `_path`
- `NOTES.md` — fold Feature 5 AND/OR section: `path()` is the OR primitive, uses existing block-keys mechanism

## Open Questions

1. Separator `/` (locked — fs-like reuse requires it)
2. Unkeyed branches inside `path() {}`: error, or auto-index (`_0`, `_1`)? Auto-index keeps `_path` non-null so patterns always match.
3. Reserved column name `_path` vs `path`? `_path` avoids user-capture collision.
4. When `path()` appears without a prior fs-producing tag, what's the cursor's starting path? `/` (root)?
5. Can `path()` appear inside `check` SQL, or only in `rule` bodies? Rule-only for v1.

## Verification

- Parser unit test: `rule r { path() { a: fs(x); b: fs(y) } }` → `Tag::Path` with two keyed slots
- Parser unit test: `> path(/rust/**)` → `Tag::Path` with pattern arg
- Lowering unit test: nested `path() {}` → N monomorphs each with `_path` MatchDef holding the concatenated key chain
- Integration (kitchen_sink): labeled `path()` rule + two checks filtering on `_path` LIKE patterns, both PASS
- SQL smoke: `SELECT _path, COUNT(*) FROM classify_data GROUP BY _path` returns one row per branch
- LSP hover on captures inside a `path()` branch shows `$_path` value
