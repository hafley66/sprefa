# Plan: LSP loop-justification lint (sprf)

## 0. Justification comment schema

```
// loop-justify: invariant=<text>; side-effect=<text>; throughput=<text>
```

- Single line. Leading whitespace allowed. Trailing whitespace allowed.
- All three keys required, separated by `;`. Values are free-form non-`;` text.
- Anchor regex: `^\s*//\s*loop-justify:\s*invariant=[^;]+;\s*side-effect=[^;]+;\s*throughput=[^;]+\s*$`
- Token name: `loop-justify` (kebab, easy to grep).

## A. Parser strategy — pick ONE seam, not two

ast-grep RuleCore supports `kind: line_comment`, `regex: ...`, and `follows: { ... stopBy: neighbor }` (confirmed in `ast-grep-config-0.36.3/src/lib.rs:211-213` and `rule/stop_by.rs`). The `precedes:` form is dogfooded at `v4/tests/ast_yaml_smoke.rs:104-139` and `v4/examples/dogfood-rust-doc-lsp.sprf`.

That means the corpus is scanned ONCE per ast-grep rule. **Do not introduce a `re` pre-pass + ast pass + join**. Use `ast_yaml(:rs)` exclusively. Two ast_yaml rules:

1. `all_loops` — every loop/iter-adapter occurrence with `(FS, LO, HI)`.
2. `justified_loops` — same set, BUT with a `follows: { kind: line_comment, regex: "^\\s*//\\s*loop-justify:...", stopBy: neighbor }` constraint.

Antijoin (C) = `all_loops \ justified_loops`.

## B. Construct inventory — single rule, `any:` table

Per `v4/examples/runtime-map-html.sprf:38-47`, ast_yaml's `any:` accepts a list of patterns under one rule. One sprf rule body, one ast pass, N candidate patterns. The corpus fans out once.

```sprf
# Storage types:
rule(:all_loops,         FS?, LO?, HI?);   # every loop occurrence
rule(:justified_loops,   FS?, LO?, HI?);   # same set with neighbor comment
rule(:unjustified_loops, FS?, LO?, HI?);   # antijoin output ⇒ diagnostics
```

Loop-pattern table (excerpt; full body lists every adapter from the brief):

```yaml
# inside  ast_yaml(:rs)`
rule:
  any:
    - { kind: for_expression }
    - { kind: while_expression }
    - { kind: loop_expression }
    - pattern: "$$$.map($$$)"
    - pattern: "$$$.filter($$$)"
    - pattern: "$$$.fold($$$)"
    - pattern: "$$$.reduce($$$)"
    - pattern: "$$$.for_each($$$)"
    - pattern: "$$$.collect::<$$$>()"
    - pattern: "$$$.collect()"
    - pattern: "$$$.iter()"
    - pattern: "$$$.iter_mut()"
    - pattern: "$$$.into_iter()"
    - pattern: "$$$.flat_map($$$)"
    - pattern: "$$$.filter_map($$$)"
    - pattern: "$$$.zip($$$)"
    - pattern: "$$$.chain($$$)"
    - pattern: "$$$.take($$$)"
    - pattern: "$$$.skip($$$)"
    - pattern: "$$$.scan($$$)"
    - pattern: "$$$.take_while($$$)"
    - pattern: "$$$.skip_while($$$)"
    - pattern: "$$$.windows($$$)"
    - pattern: "$$$.chunks($$$)"
    - pattern: "$$$.step_by($$$)"
    - pattern: "$$$.cycle()"
    - pattern: "$$$.repeat($$$)"
    - pattern: "$$$.count()"
    - pattern: "$$$.sum()"
    - pattern: "$$$.product()"
    - pattern: "$$$.max()"
    - pattern: "$$$.min()"
    - pattern: "$$$.any($$$)"
    - pattern: "$$$.all($$$)"
    - pattern: "$$$.find($$$)"
    - pattern: "$$$.position($$$)"
    - pattern: "$$$.find_map($$$)"
    - pattern: "$$$.enumerate()"
    - pattern: "$$$.peekable()"
    - pattern: "$$$.rev()"
    - pattern: "$$$.par_iter()"
    - pattern: "$$$.par_iter_mut()"
    - pattern: "$$$.into_par_iter()"
    - pattern: "$$$.par_bridge()"
`
```

The "stdlib hot calls nested INSIDE any of the above" requirement (Vec::new, format!, ...) is a SECOND, optional rule using `inside:` against any of the loop kinds. Phase 2.

## C. Antijoin shape (full-SQL kind)

Per `v4/tests/antijoin_fuse_target.rs:30-65` the canonical antijoin lowers to full-SQL when the body is pure rule-query + `not(...)`:

```sprf
rule(:all_loops, FS?, LO?, HI?) {
    fs > glob`**/*.rs` > ast_yaml(:rs)`<any: pattern table from B>`
};

rule(:justified_loops, FS?, LO?, HI?) {
    fs > glob`**/*.rs` > ast_yaml(:rs)`
        rule:
          all:
            - any:
                - { kind: for_expression }
                - { kind: while_expression }
                - { kind: loop_expression }
                - { pattern: "$$$.map($$$)" }
                # ... full table inline ...
            - follows:
                kind: line_comment
                regex: "^\\s*//\\s*loop-justify:\\s*invariant=[^;]+;\\s*side-effect=[^;]+;\\s*throughput=[^;]+\\s*$"
                stopBy: neighbor
    `
};

rule(:unjustified_loops, FS?, LO?, HI?) {
    all_loops?(FS?, LO?, HI?) > not(justified_loops?(FS, LO, HI))
};

unjustified_loops?(FS?, LO?, HI?)
  > lsp_error(:loop-unjustified)`loop without justification at ${FS}:${LO}`;
```

`stopBy: neighbor` is the ast-grep default — comment must be the immediately preceding sibling. Tree-sitter parses `// foo\nfor ...` such that the comment IS the immediate-prior sibling of the loop's enclosing statement; matches `v4/tests/ast_yaml_smoke.rs:104-139` precedent (`///` + `fn`).

## D. "Comment on line above" — single ast pass, no regex pre-pass

ast-grep's `follows: { ... stopBy: neighbor }` is the correct primitive. **No regex pass needed.** The fallback regex+join is rejected because (1) it doubles the corpus scan, (2) requires a join keyed on `(FS, prev_line_range)` which sprf cannot express without a custom op, and (3) violates the "no per-row sqlite scan" scaling rule.

## E. Scaling

- `all_loops`: 1 ast_yaml pass over `**/*.rs`.
- `justified_loops`: 1 ast_yaml pass over `**/*.rs` (same pattern table + `follows`).
- `unjustified_loops`: full-SQL antijoin, NOT EXISTS subquery — single SQLite statement.
- `lsp_error` consumes `unjustified_loops` rows. One diag per row.

Critical: keep `fs > glob > ast_yaml` ordering inside each rule body. **Never** put a `rule_name?(...)` read before `fs` (sprf surface constraint).

## F. Test plan

Fixture: `v4/examples/dogfood-loop-justify-target.rs`:
```rust
// (a) properly justified
// loop-justify: invariant=size bounded by file count; side-effect=none; throughput=O(n)
for path in paths { drop(path); }

// (b) unjustified
for x in xs { drop(x); }

// (c) iterator chain, unjustified
let _ = (0..10).map(|i| i + 1).filter(|i| *i > 3).collect::<Vec<_>>();

// (d) nested: outer justified, inner bare
// loop-justify: invariant=N<1024; side-effect=writes log; throughput=O(n^2) acceptable
for row in rows {
    for col in row.cols() { /* inner is bare */ }
}
```

Expected diagnostics:
- (b) `loop-unjustified` at `for x in xs`.
- (c) three diags: `.map`, `.filter`, `.collect::<Vec<_>>()`.
- (d) one diag on inner `for col in row.cols()`.

Test file: `v4/tests/lsp_loop_justify_target.rs` — model on `v4/tests/lsp_locate_dsl_smoke.rs` + `v4/tests/ast_yaml_smoke.rs:104-139`. Assert diagnostic counts per fixture + codes equal `loop-unjustified`.

## G. Open questions

1. **Adapter chain granularity**: `.map(...).filter(...).collect()` = ONE site or THREE? Plan above emits three.
2. **`Vec::from_iter(...)` / `iter::once(...)` / `iter::repeat(...)`**: include?
3. **Fn-level umbrella comment**: does `// loop-justify: ...` on a `fn` license every loop in the body? Plan defaults NO.
4. **Macro-generated loops** (`vec![x; n]`, custom `for_each!` macros): not linted in v1.
5. **Justified outer + bare inner (case d)**: confirmed correct by `stopBy: neighbor`. User should confirm intent.
6. **Where the lint runs**: standalone sprf script invoked by CI, OR wired into `sprefa-lsp` live diagnostics? Plan assumes standalone.

## TODO checklist

- [ ] Resolve open questions 1, 3, 4 before writing the pattern list.
- [ ] Create `v4/examples/dogfood-loop-justify-target.rs` (fixture above).
- [ ] Create `v4/examples/dogfood-loop-justify-lsp.sprf` (rules from C, full pattern table from B).
- [ ] Create `v4/tests/lsp_loop_justify_target.rs` (model on `ast_yaml_smoke.rs` + `antijoin_fuse_target.rs`).
- [ ] Confirm `ast_yaml` accepts a top-level `any:` + `follows:` mix. If not, nest under `all:` as in section C.
- [ ] Verify `lsp_error` (no `[TERM]` form) anchors at the matched node's coord — `v2_ops.rs:1196-1212` shows `ast_yaml` sets focal span via `stamp_source_value`.
- [ ] Run on the v4 repo itself as final dogfood gate.

## Critical Files for Implementation

- `v4/examples/dogfood-loop-justify-lsp.sprf` (new — the sprf program)
- `v4/examples/dogfood-loop-justify-target.rs` (new — fixture)
- `v4/tests/lsp_loop_justify_target.rs` (new — integration test)
- `v4/src/v2_ops.rs` (reference — `AstYamlComponent` at line 1103)
- `v4/src/lsp.rs` (reference — `lsp_error`/`lsp_warn` surface)
