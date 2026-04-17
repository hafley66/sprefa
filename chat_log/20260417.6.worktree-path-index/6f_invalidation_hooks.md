# 6f — Invalidation hooks (moving refs)

Content-addressed revs (tags, SHAs, semver tags) are immutable — never
invalidate. Moving refs (`HEAD`, branch names) need `drop_rev` on ref-move
plus `drop_wt` on the provisioner.

## Classifier

```rust
# v2/src/readers/_6_rev_class.rs
pub enum RevKind { Sha, Tag, Branch, Head, Unknown }

pub fn classify(locator: &dyn CheckoutLocator, repo: &str, rev: &str) -> RevKind {
    # exact hex sha?  → Sha
    # in refs/tags/?  → Tag
    # in refs/heads/? → Branch
    # == HEAD ?       → Head
    # else            → Unknown
}

impl RevKind {
    fn is_mutable(&self) -> bool {
        matches!(self, RevKind::Branch | RevKind::Head | RevKind::Unknown)
    }
}
```

## Watcher integration

The existing watcher observes `.git/objects/pack` and ref changes. On a
ref-move event for a tracked branch:

```rust
# v2/src/watcher/... (existing)
on_ref_move(repo: &str, ref_name: &str, old_oid, new_oid) {
    if !classify(locator, repo, ref_name).is_mutable() { return }
    path_index.drop_rev(repo, ref_name);
    if let Some(p) = provisioner { p.drop_rev(repo, ref_name); }
    # next fs() call re-provisions + re-indexes with the new tree
}
```

## Tag move case

Tags technically moveable (`git tag -f`). Rare but legal. Two options:

- Conservative: treat `Tag` as mutable too — loses immutable-tag speedup
- Opportunistic: trust tags until caller explicitly `drop_rev`s

Pick opportunistic. Document in `RuntimeConfig` comment. Add `sprefa admin
drop-rev <repo> <rev>` CLI for manual eviction.

## HEAD suffix `$wt`

`project_v2_wt_overlay.md` reserves `$wt` as a rev suffix meaning "current
working tree overlay." Provisioner treats `$wt` as "link to checkout, don't
materialize," same as today's WT layer. drop on buffer change (existing).

## Blast radius

- `v2/src/readers/_6_rev_class.rs` — new file, ~40 lines
- `v2/src/watcher/*` — hook on_ref_move (identify existing path)
- `v2/src/bin/sprefa_v2.rs` — optional `admin drop-rev` subcommand
- Tests: branch move evicts + re-provisions; tag move sticks unless forced

## Depends on / depended on by

- Depends: 6c (drop_rev), 6b (drop_rev)
- Depended on: none — this closes the loop
