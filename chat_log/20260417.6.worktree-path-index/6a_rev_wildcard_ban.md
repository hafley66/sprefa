# 6a — rev wildcard ban

Bound the materialization set by rejecting unbounded rev patterns at parse.

## Types

```rust
# v2/src/ops/_2_rev.rs
enum RevDiag {
    UnboundedWildcard { site: ParseSite, pattern: Arc<str> },
    // ... existing variants
}

impl Diagnostic for RevDiag { /* ... */ }
```

## Shape

```rust
# RevOp constructor (parse-time validation)
impl RevOp {
    fn from_args(args: &[Arg], site: ParseSite, diags: &DiagSink) -> Option<Self> {
        let pat = parse_glob_or_literal(args)?;
        if is_unbounded(&pat) {
            diags.0(Box::new(RevDiag::UnboundedWildcard {
                site, pattern: pat.src.clone(),
            }));
            return None;        // pipeline refuses to lower
        }
        Some(Self { pat, site })
    }
}

fn is_unbounded(pat: &CompiledPattern) -> bool {
    matches!(pat.src.as_ref(), "*" | "**" | "**/*" | "*/*" )
    // bare wildcard, no prefix or suffix literal
}
```

## Diagnostic body

```
rev pattern `**` matches every rev unconditionally. Each rev materializes
a git worktree on first query. Narrow with a prefix/suffix so the set is
bounded:
    rev(v1.*)      # tags starting with v1.
    rev(*-stable)  # tags ending in -stable
    rev(main)      # literal
```

## Blast radius

- `v2/src/ops/_2_rev.rs` — add variant, guard in constructor, test
- `v2/src/_8_parse.rs` — nothing if rev ctor path runs through host_parse
- Tests: one positive (`rev(v1.*)` passes), one negative (`rev(**)` emits
  diag, pipeline empty)

## Depends on / depended on by

- Independent. Lands standalone.
- 6b / 6c / 6d rely on the invariant that rev cursors are a bounded set.
