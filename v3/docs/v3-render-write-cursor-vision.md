# v3 render / write_cursor / lsp[severity] vision

Captured 2026-04-26. Design dialogue, not landed code. Companion to
`v3-unified-language-locks.md` and `v3-semantic-model.md`.

## Core thesis

Every cursor carries `(content, byte_range, fs, repo, rev, captures)`.
That tuple is both a read address (what produced this cursor) and a
write address (where output can be spliced). The output side needs
exactly two new pieces:

- `render[fmt]` — a universal output op family. Format picks the dialect
  (md / ascii / lsp / rust / json / sql). Bracket-arg as discriminator,
  same shape as `ast[lang]` and `cst[lang]`.
- `write_cursor` — a sink op that splices its input cursor's bytes into a
  target cursor's `byte_range`. Backed by a single `WriteRangeEffect`.

Everything else is composition over what already exists.

## render[fmt]

```sprf
render[md]   (template)   # markdown
render[ascii](template)   # ASCII diagram (boxes / arrows / flow)
render[lsp][severity](template)   # LSP diagnostic / hover / code action
render[rust] (template)   # codegen — emits source text
render[json] (template)   # structured payload
```

Template body is `str`-style with bound captures interpolated. `[fmt]`
selects a tiny minilang per backend (md tables, ANSI cells, LSP
`Diagnostic{range, severity, message}`, json schema).

Backtick strings — `` `foo ${X}` `` — are the JS-template-literal form
for inline templates. Backtick is currently unclaimed in the host
grammar; `${...}` inside re-enters the existing `carveout_expr`. Read
mode only inside backticks (output position, not bind position).

## write_cursor (positional, no kwargs)

```sprf
... > render[md](- {fs.stem}) > write_cursor(${SCOPE});
... > render[rust](use crate::${M};) > write_cursor(${SCOPE});
... > render[md](`# ${TITLE}\n${BODY}`) > write_cursor(${SCOPE}, :append);
```

Args are positional:

1. target cursor (must carry `byte_range`)
2. mode atom — defaults to `:replace`. Other modes: `:append`,
   `:prepend`, `:wrap`.

No keyword args. Bashism `<<` redirect was considered and rejected:
sprf's pipe direction is `>` left-to-right, sinks sit at the right end,
adding `<<` introduces a second statement form with reversed flow for
no real terseness gain. If sugar ever earns its weight, land it later
as a parser-level desugar to the op call (same status as `:atom`
sugar for `Value::Atom`).

Backed by one effect:

```rust
pub struct WriteRangeEffect {
    pub file: PathBuf,
    pub byte_range: Range<usize>,
    pub new_bytes: Bytes,
    pub mode: WriteMode,  // Replace | Append | Prepend | Wrap
}
```

Same effect powers `write_file(./MAP.txt)` (range = whole file) and any
future LSP code-action edit. Idempotent on re-run because byte_range is
recomputed each pass. Fingerprinted, batched at drain.

## lsp[severity]

```sprf
lsp[error] (range = ${R}, message = "unwrap in production path")
lsp[warn]  (range = ${R}, message = "...")
lsp[hint]  (range = ${R}, message = "...")
lsp[info]  (range = ${R}, message = "...")
```

Bracket-arg is the severity discriminator. Same family covers hover and
code actions via additional bracket-arg slots when those land. Backend:
`DocSession` has the diag channel; add an `lsp[*]` collector that
publishes per-doc. The user writes lints in sprf, hot-reloads on save,
no rebuild.

## Programmable LSP — the why

```sprf
fs(glob(**/*.rs))
  > ast[rust](unwrap())                # cursor.byte_range = the unwrap call
  > lsp[warn](
      range   = ${HERE.byte_range},
      message = "unwrap in production path",
      code    = "no-unwrap")
;
```

Cursor already IS the diagnostic minus the message. No new plumbing.
Code-action variant adds an `edit = render[rust](...)` slot that maps
to a `WriteRangeEffect`.

## Comment markers — bidirectional carveouts

```sprf
# read direction: sprf in comment is input
comment(@sprf ${BODY?}) > eval(${BODY});

# write direction: comment-bounded region is output target
comment(@begin ${SCOPE?} @end);
fs(glob(./views/*/))
  > render[md](- {fs.stem})
  > write_cursor(${SCOPE});
```

`SCOPE` was bound by the comment op finding the `@begin … @end` pair;
its `byte_range` covers the body between the markers. Re-run is
idempotent because the marker pair is rediscovered each pass and
byte_range recomputed.

That closes the loop: code → sprf → code, in the same file, reactive
on save. Surgical macro-like codegen placed in code, no build step.

## tag — variadic write / nullary read

Replaces v2 scan-pointer.

```sprf
# write: assert a fact, re-fires when any binding changes
tag(:prod-cut, ${REPO?}, ${REV?});

# read: subscription source, emits each binding tuple
tag(:prod-cut) > strings_index(${REPO}, ${REV});
```

Variadic columns named by the carveout name. Datalog-with-reactive-
semantics. Filesystem and git events feed the Pending channel
(sprefa-126 / sprefa-bsa); ghcacher / fs-watcher / LSP didChange all
push into the same primitive.

## rule — wide SQLite table, batched

Coming back from v0/v1. Every rule auto-schemas a table from its capture
set; every cursor a rule emits is one row; flush is a `Batch` effect,
**never inline, never in a loop**. The Store trait + `effect_runtime`
already has the batching contract.

Missing pieces (gated on port-or-revive of `crates/sprefa`, see
v3-workspace-boundary memory):

- port `Store` / `Batch` / `ExprTableSpec` / `CaptureColumn` / `NoopStore`
  into the live workspace
- re-implement `RuleOp` against `pipeline::Op` (v3 trait, not v2)
- auto-schema lowering: union of all `binds_captures()` across rule body
  → `ExprTableSpec`
- Pass-A registers schemas, Pass-B streams INSERTs into Batch, runner
  flushes at drain points

Hot path stays pure Rust byte-loop. SQLite touched only at batch
boundaries. Same fingerprint / cache contract as `sh[]`.

## Sigil consolidation

Drop `&{...}` for entity addressing. Once parametric rules land,
`host_parse(crate=:sprefa_parse)` IS the entity reference — rule name is
the global symbol, args narrow it. Symbol table = registered rules per
file context. `&{...}` stays reserved for future address-shaped use or
gets retired entirely.

Net sigil count after this lock:

- `${X?}` host carveout (read / unbound modes)
- `${{shell}}` shell literal (atomic)
- `:atom` atom literal
- `` `template` `` backtick string (NEW)
- `"string"` / `r"raw"` / `r#"raw#"#` string literals
- `# comment`

No `&`, no `<<`, no `>>`, no `$$`.

## The whole picture

```
   sources                       ops                            sinks
   ───────                       ───                            ─────
   .sprf files          fs / glob / ast / cst / comment    render[md]   ─► write_file
   @sprf in comments    re / json / yaml / toml            render[ascii]─► print
   tag subscriptions    tag(read) / collect                render[lsp]  ─► LSP channels
                        rule (auto-schema)                 render[rust] ─► write_cursor(:replace)
                                                           render[json]
                                                           rule sink    ─► Store::flush_batch
                                                                            (SQLite, batched)
```

## Open work

Three gating items, in order:

1. `crates/sprefa` port-or-revive (Store / RuleOp) — already known,
   precursor to `sprefa-4m7.7.11` and Store-dep cards
2. Pending primitive (`sprefa-126`) — for `tag` subscribe + LSP
   reactivity. Q1 = A locked (Pending IS runner-layer primitive,
   tag-subscribe is one client). Q2-Q5 still open.
3. `WriteRangeEffect` plumbing — the one effect that powers
   `write_cursor`, `write_file` range-mode, and LSP code-action edits.

After those land, additive: `render[md]`, `render[ascii]`, `render[rust]`,
`render[json]`, `lsp[severity]`, backtick string token, `collect` op,
`comment` body re-entry, `eval` op.

## What this enables

- Polyglot `@sprf`-in-comment surgical codegen (auto-imports, render-
  folder-names-into-doc-block, type-transclusion-into-doc-region)
- Programmable LSP (lints, hovers, code actions all written in sprf,
  hot-reloaded on save)
- Reactive maps / diagrams of repo/rev state, written to disk or shown
  in LSP virtual docs
- v0/v1 strings / norm / norm2 SQLite extraction reduced to a ~15-line
  sprf recipe (fs > ast[lang](string_literal ${S}) > rule sink)
- Cross-repo entity tagging via tag(:prod-cut, repo, rev) replacing the
  v2 scan-pointer dance
