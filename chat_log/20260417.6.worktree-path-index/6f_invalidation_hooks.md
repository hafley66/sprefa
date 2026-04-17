# 6f — Invalidation hooks (Z3)

## Scope for this session

Watcher does not exist in v2 yet (`v2/CLAUDE.md` lists it under "later").
6f reduces to:

1. Confirm the invalidation API surface is exposed on trait + struct.
2. Add a `RevKind` classifier as a standalone module for the future hookup.
3. Leave a one-line `TODO` in the watcher placeholder pointing here.

No runtime behavior change. This task closes with ~80 LOC plus tests.

## What already exists (from 6b + 6c)

- `PathIndex::drop_rev(repo: &str, rev: &str) -> Result<(), PathIndexErr>`
- `WorktreeProvisioner::drop_rev(repo: &str, rev: &str) -> Result<(), ProvErr>`

Both async. Both idempotent on missing key. Both tested in their respective
files. No additions needed to land 6f — only the classifier and doc.

## Step 1 — RevKind classifier

```rust
# v2/src/readers/_7_rev_class.rs   (pick next free prefix)
//! Classify a rev spec into mutable / immutable. Drives whether the
//! watcher should call `drop_rev` on a ref-move event.

use super::_1_locator::CheckoutLocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevKind {
    /// 40-hex SHA. Immutable by construction.
    Sha,
    /// Resolved via `refs/tags/`. Treated as immutable (opportunistic;
    /// `git tag -f` is rare and requires explicit admin drop_rev).
    Tag,
    /// Resolved via `refs/heads/`. Mutable; drop on ref-move.
    Branch,
    /// Literal `HEAD`. Mutable; drop on ref-move.
    Head,
    /// Not resolvable via the locator's repo. Conservatively mutable.
    Unknown,
}

impl RevKind {
    pub fn is_mutable(self) -> bool {
        matches!(self, RevKind::Branch | RevKind::Head | RevKind::Unknown)
    }
}

/// Classify. Pure — no I/O beyond the locator's existing repo probes.
pub fn classify(locator: &dyn CheckoutLocator, repo: &str, rev: &str) -> RevKind {
    if rev == "HEAD" { return RevKind::Head; }
    if is_hex_40(rev) { return RevKind::Sha; }

    # The locator exposes `locate(repo, rev) -> Option<PathBuf>` and
    # `revs(repo) -> Vec<Arc<str>>`. Use existing methods; do NOT
    # extend the trait. If the locator can't tell tag vs branch without
    # opening the repo, return Unknown.
    let Some(repo_path) = locator.locate(repo, "HEAD") else {
        return RevKind::Unknown;
    };

    # Fastest shell probe without new deps:
    let is_branch = std::process::Command::new("git")
        .args(["show-ref", "--verify", "--quiet",
               &format!("refs/heads/{rev}")])
        .current_dir(&repo_path)
        .status().map(|s| s.success()).unwrap_or(false);
    if is_branch { return RevKind::Branch; }

    let is_tag = std::process::Command::new("git")
        .args(["show-ref", "--verify", "--quiet",
               &format!("refs/tags/{rev}")])
        .current_dir(&repo_path)
        .status().map(|s| s.success()).unwrap_or(false);
    if is_tag { return RevKind::Tag; }

    RevKind::Unknown
}

fn is_hex_40(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b|
        matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F'))
}
```

## Step 2 — Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha_is_immutable() {
        # classify without a real locator: use a stub that returns None
        # from locate() — SHA path doesn't need the locator anyway.
        let stub = NoopLocator;
        assert_eq!(classify(&stub, "r", "0123456789abcdef0123456789abcdef01234567"),
                   RevKind::Sha);
        assert!(!RevKind::Sha.is_mutable());
    }

    #[test]
    fn head_is_mutable() {
        let stub = NoopLocator;
        assert_eq!(classify(&stub, "r", "HEAD"), RevKind::Head);
        assert!(RevKind::Head.is_mutable());
    }

    #[test]
    fn unknown_is_mutable() {
        let stub = NoopLocator;   # returns None from locate
        assert_eq!(classify(&stub, "r", "something-weird"), RevKind::Unknown);
        assert!(RevKind::Unknown.is_mutable());
    }

    # Branch/Tag tests require a fixture repo; mirror the 6b pattern:
    #[tokio::test]
    async fn branch_then_tag_via_fixture() {
        let fixture = make_fixture_repo_with_branch_and_tag();
        let locator = SingleRepoLocator::new(fixture.path());
        assert_eq!(classify(&locator, "r", "main"), RevKind::Branch);
        assert_eq!(classify(&locator, "r", "v1.0.0"), RevKind::Tag);
    }

    struct NoopLocator;
    impl CheckoutLocator for NoopLocator {
        fn locate(&self, _: &str, _: &str) -> Option<std::path::PathBuf> { None }
        fn repos(&self) -> Vec<std::sync::Arc<str>> { vec![] }
        fn revs(&self, _: &str) -> Vec<std::sync::Arc<str>> { vec![] }
    }
}
```

## Step 3 — Register in readers/mod.rs

```rust
pub mod _7_rev_class;   # or next free prefix
pub use _7_rev_class::{RevKind, classify};
```

## Step 4 — Watcher placeholder doc

The watcher module doesn't exist. When it's added (future session S3 per
`v2/CLAUDE.md`), the integration point looks like:

```rust
# Future: v2/src/watcher/mod.rs (not in this session)
on_ref_move(repo: &str, ref_name: &str, _old: Oid, _new: Oid) {
    use crate::readers::{classify, RevKind};
    if classify(&*locator, repo, ref_name).is_mutable() {
        let _ = path_index.drop_rev(repo, ref_name).await;
        if let Some(p) = &provisioner {
            let _ = p.drop_rev(repo, ref_name).await;
        }
    }
}
```

Until then, no caller of `drop_rev` lands. Manual eviction is possible via
a future `sprefa admin drop-rev <repo> <rev>` CLI (out of scope).

## Absolute stop conditions

- Building any watcher code — that's a different session.
- Extending `CheckoutLocator` trait.
- Calling `drop_rev` from anywhere in production this session.
- Adding deps.

## Blast radius

| file | change | lines |
|---|---|---|
| `v2/src/readers/_7_rev_class.rs` | new classifier + tests | +110 |
| `v2/src/readers/mod.rs` | module decl + re-export | +2 |

## Verify

```
cd v2 && cargo build --tests 2>&1 | tail -20
cd v2 && cargo test -p v2 --lib rev_class 2>&1 | tail -10
cd v2 && cargo test -p v2 --lib 2>&1 | tail -5   # ≥254 passed (250 + 4 new)
```

## Depends on / depended on by

- Depends: 6b, 6c (trait surfaces landed; nothing new required).
- Depended on: future S3 watcher session.
