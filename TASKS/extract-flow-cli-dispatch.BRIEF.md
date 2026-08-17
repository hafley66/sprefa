# extract-flow-cli-dispatch

Issue: `issues/extract-flow-cli-dispatch/item.md` (epic soopy-full-wiring).
Repo: `~/projects/sprefa`. All paths below are repo-relative.

## First action

```bash
git merge --ff-only 4531b429769e81b6a1142fdb232805c155836335
```

Failure = STOP AND REPORT. Do not work around it.

## Ownership

You own ONLY:
- `v6/sprefa-extract/src/bin/extract.rs`
- `v6/sprefa-extract/src/bin/extract/help.rs`
- `v6/sprefa-extract/src/project.rs`
- `v6/sprefa-extract/tests/**`

FORBIDDEN, do not open, do not edit, do not create files under:
`v6/sprefa-extract/src/lang/**`, `v6/sprefa-extract/src/types.rs`,
`v6/sprefa-extract/src/wire.rs`, `v6/sprefa-extract/src/family.rs`,
`v6/sprefa-extract/src/lib.rs`, `v6/sprefa-engine-rs/**`, `v6/tsv2/**`,
`v6/prolog/**`, `issues/**`, `plans/**`, `chat_log/**`, `TASKS/**`,
`CLAUDE.md`, `ARCH.pl`. Everything else in the repo is a sibling lane's.

Every function you need already exists and is already exported. You add ZERO
new library functions.

## Background, all receipts

`FlowF` landed in PR #313. The join is written and tested and has zero
production callers:

| thing | where |
|---|---|
| `pub fn flow_edges(inputs: &[(ContentId, &ExtractOutput)], resolved: &[(ContentId, Vec<ProjectEdge<CallF>>)]) -> Vec<FlowEdge>` | `src/types.rs:773` |
| `pub fn flatten_flow(edges: &[FlowEdge]) -> Vec<FlatFact>` | `src/wire.rs:279` |
| both re-exported | `src/lib.rs:52` and `src/lib.rs:96` |
| the join's unit test (drives `flow_edges` directly, no CLI) | `tests/13_flow_join.rs` |
| `struct ResolveArms { call, types }` | `src/project.rs:45-52` |
| `fn resolve_project` | `src/project.rs:139` |
| `fn call_facts(input, inputs, cx) -> Vec<FlatFact>` | `src/project.rs:629` |
| `fn resolve_call_edges(path, output, cx) -> Vec<ProjectEdge<CallF>>` | `src/project.rs` (grep it; `call_facts:633` calls it) |
| `fn parse_arms(families) -> Result<ResolveArms, String>` | `src/bin/extract.rs:472` |
| `fn parse_mask(families) -> Result<FamilyMask, String>` | `src/bin/extract.rs:492` |

## PINNED DECISION, do not re-decide

**`flow` is a RESOLVE ARM, not a `FamilyMask` bit.** The issue text says
"add the flow name to parse_mask"; that is superseded. Receipt:
`src/types.rs:722-724` states FlowF is "Phase-2 only: no `FamilyMask` bit, no
`ExtractOutput` field; a pure join computes its edges", and `FamilyMask` has no
`flow` field. `flow_edges` consumes RESOLVED call edges, which only exist under
`--resolve`. So the door is `parse_arms` + `ResolveArms`, and `parse_mask` is
UNCHANGED except as noted in Task 4.

## Task 1: `ResolveArms` gains a `flow` arm

In `src/project.rs:45-52`, add a third field with a doc comment in the same
voice as its two neighbours:

```rust
    /// `FlowF`: the inter-procedural value-flow join over resolved call edges.
    /// A pure join, so it needs the `call` resolve to have run and emits
    /// whole-project edges rather than per-file rows.
    pub flow: bool,
```

## Task 2: `resolve_project` dispatches the join

Currently `src/project.rs:190-198` reads:

```rust
    let mut facts = Vec::new();
    for input in &inputs {
        if request.arms.call {
            facts.extend(call_facts(input, &inputs, &cx));
        }
        if request.arms.types {
            facts.extend(type_facts(input, &inputs, &cx));
        }
    }
    Ok(facts)
```

Replace with a shape that resolves call edges AT MOST ONCE per input even when
both arms are on (never resolve the same input twice: that is the N+1 law
applied to work, not rows):

```rust
    let resolved_calls: Vec<(ContentId, Vec<ProjectEdge<CallF>>)> =
        if request.arms.call || request.arms.flow {
            inputs
                .iter()
                .map(|input| {
                    (
                        input.blob.clone(),
                        resolve_call_edges(&input.path, &input.output, &cx),
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

    let mut facts = Vec::new();
    if request.arms.call {
        for (input, (_, edges)) in inputs.iter().zip(resolved_calls.iter()) {
            facts.extend(call_facts(input, &inputs, edges));
        }
    }
    for input in &inputs {
        if request.arms.types {
            facts.extend(type_facts(input, &inputs, &cx));
        }
    }
    if request.arms.flow {
        facts.extend(flatten_flow(&flow_edges(&pairs, &resolved_calls)));
    }
    Ok(facts)
```

and change `call_facts` (`src/project.rs:629`) to take the already-resolved
edges instead of `cx`:

```rust
fn call_facts(
    input: &ProjectInput,
    inputs: &[ProjectInput],
    edges: &[ProjectEdge<CallF>],
) -> Vec<FlatFact> {
    let Some(call) = input.output.call.as_ref() else {
        return Vec::new();
    };
    edges
        .iter()
        .filter_map(|edge| {
```

The rest of `call_facts`'s body is UNCHANGED (delete only the
`resolve_call_edges(&input.path, &input.output, cx)` head and the `.iter()`
that followed it). `call_facts` has no other caller; grep to confirm before
you edit.

`pairs` already exists at `src/project.rs:143-147` with exactly the type
`flow_edges` wants. Import `flatten_flow` / `flow_edges` / `FlowEdge` /
`ProjectEdge` / `CallF` through the crate's own module paths at the top of
`project.rs`, matching how the neighbouring imports there are already spelled.

**Fact ORDER does not matter**: `resolve_project_jsonl`
(`src/project.rs:267-269`) sorts lines.

## Task 3: `parse_arms` accepts `flow`

`src/bin/extract.rs:472-489`. Add the arm and widen the error text:

```rust
            "call" => arms.call = true,
            "type" | "types" => arms.types = true,
            "flow" => arms.flow = true,
            other => {
                return Err(format!(
                    "--family '{other}' is not a resolve arm; under --resolve only \
                     'call', 'type' and 'flow' are meaningful"
                ))
            }
```

and widen the empty-selection guard, currently `if !arms.call && !arms.types`,
to also consider `arms.flow`. Update its message to name flow.

The `--resolve` default (`src/bin/extract.rs:453-457`) stays `call: true,
types: false` and gains `flow: false`: an existing `--resolve` invocation must
emit byte-identical output. Say so in the PR body and prove it (Task 6).

## Task 4: drift fix, the help text lies about unknown family names

`src/bin/extract/help.rs`, `FAMILY_LONG` (starts at `:135`), contains the
sentence "Unknown names are silently ignored." That is FALSE at both doors:
`parse_mask` (`extract.rs:503-508`) and `parse_arms` (`extract.rs:477-482`)
both return `Err`. Replace that sentence with one saying an unknown name is a
named error, and add `flow` to the `--resolve` paragraph's list of arms
(currently "`call` (the default) and/or `type`"). Change nothing else in that
constant; keep the line wrapping style of the surrounding text.

## Task 5: drift fix, `--family cfg --bench` drops the cfg pass

`src/bin/extract.rs:335-345`: `cfg` is computed from the family list, then
`bench(&path_str, &content, mask)` is called WITHOUT it, so `--family cfg
--bench` never runs the cfg pass and its timing line never mentions cfg.
Compare `stream` (`:513-533`), which does take `cfg: bool` and runs
`cfg_bundle` + `flatten_cfg`.

Give `bench` (`:535`) the same `cfg: bool` parameter and pass it at the call
site. Inside `bench`, when `cfg` is true, time the `cfg_bundle(path, &out,
content)` pass the way `stream` builds it and add a `cfg=<node count>` (or
`cfg_facts=<n>` if the bundle exposes no node count; read `cfg_bundle`'s return
type before choosing) term to the existing `eprintln!` summary. That
`eprintln!` is the pre-existing `@eprintln-ok` CLI-UX line; do not add a new
one and do not convert it.

## Task 6: tests

Add to `v6/sprefa-extract/tests/` (new file, name it in the numbered style the
directory already uses; `ls v6/sprefa-extract/tests/` first and pick the next
free number):

1. `parse_arms` accepts `flow`, and rejects a bogus name with an error message
   naming flow. If `parse_arms` is private to the binary, drive it instead
   through the built binary with `--resolve --family flow` on the fixture
   corpus and assert the process succeeded; state in the test header which
   route you took and why.
2. An end-to-end `--resolve --family call,flow` run over an existing
   multi-file fixture that emits at least one `flow_edge` fact, asserting the
   fact family tag is present in the output. Find a fixture that already
   produces resolved call edges by reading the existing resolve tests in
   `v6/sprefa-extract/tests/`; do NOT invent a new corpus.
3. FAIL-PRE-FIX receipt: before your change, `--resolve --family flow` errors
   with "not a resolve arm". Capture that output and paste it in the PR body.
4. A test that `--resolve` with NO `--family` emits byte-identical output to
   before your change (golden the current output first, then compare).

## Validation, run it exactly

```bash
cd ~/projects/sprefa/v6/sprefa-extract && cargo build --release --features cli --bin extract 2>&1 | tail -5
cd ~/projects/sprefa/v6/sprefa-extract && cargo test 2>&1 | tail -40
```

Both rc=0. Run `cargo test` TWICE and put both pass/fail counts in the PR body.

## Style laws

- `tracing` only; no NEW `eprintln!` in `src/**`. The one in `bench` is
  pre-existing and carries `@eprintln-ok`.
- Comment budget: a comment states only a constraint the code cannot show. No
  change-log narrative, no dates, no arc references, no restating the next line.
  Sabotage / fail-first receipts belong in TEST headers.
- BANNED words in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source, base, critical, mode.
- The word "refusal" is banned in prose. An unbuilt construct is "TODO" or
  "not built yet".
- The language vocabulary is rxjs, prolog or SQL words. "support" is banned;
  use refCount.
- No em dashes. Descriptive names, never single letters.
- Colocated consistency: match each file's existing style.

## Out of scope, do NOT do it

The `cpg_taint_walk` golden's two derived `flow_*` dl6 rels collapsing into a
direct wire read is a FOLLOW-UP owned by the coordinator. `v6/tsv2/**` is
forbidden to you. Name it as a follow-up in the PR body and stop.

## Landing

Branch is already checked out for you. Commit with trailer
`Refs-Issue: @extract-flow-cli-dispatch`, push, and open the PR with
`gh pr create`, receipts in the body.

DO NOT merge. DO NOT push to main. You never spawn subagents.
