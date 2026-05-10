---
name: sprf-lsp-staged-writes
description: [v4 planning] Staging writes through a pure effect interpreter and surfacing them via LSP (inlay, code lens, code action, applyEdit). Time-travel via journal layer. Load when implementing auto-refactor, dry-run, or write-preview features.
---

# Staged writes + LSP rendering tiers

## Pattern: pure effect + interpreter swap

User code calls `ctx.put(WriteEffect{...}).await`. The runtime routes to a registered batcher. Swapping the batcher swaps the policy without touching call sites.

```
                  ┌────────────────────────────────────┐
                  │  user code (Op / saga)              │
                  │   ctx.put(WriteEffect {            │
                  │      path: "x.rs",                 │
                  │      range: 10..20,                │
                  │      bytes: "new",                 │
                  │   }).await                         │
                  └────────────────┬───────────────────┘
                                   │  same call site
                                   │  no branching in user code
                                   ▼
                  ┌────────────────────────────────────┐
                  │  RtCtx routes WriteEffect to a     │
                  │  registered batcher                │
                  └────────────────┬───────────────────┘
                                   │
              ┌────────────────────┼─────────────────────┐
              ▼                    ▼                     ▼
   ┌──────────────────┐ ┌─────────────────────┐ ┌─────────────────────┐
   │ FsWriteBatcher   │ │ StagingBatcher      │ │ DryRunBatcher       │
   │  (real)          │ │  (buffered)         │ │  (count + drop)     │
   │                  │ │                     │ │                     │
   │ tokio::write()   │ │ pushes WriteEdit    │ │ inc counter,        │
   │ → returns Ok     │ │ to in-memory        │ │ returns Ok          │
   │                  │ │ Vec<WriteEdit>,     │ │                     │
   │                  │ │ returns Ok          │ │                     │
   └──────────────────┘ └──────────┬──────────┘ └─────────────────────┘
                                   │
                                   ▼
                        Vec<WriteEdit> exposed
                        as Store on RtCtx
                                   │
                                   ▼
                        LSP reads it, renders
```

For "stage writes into comment areas": a staging batcher that wraps writes into `// sprefa: would-write [bytes]` comments is one batcher impl, ~50 LoC.

## Three LSP rendering tiers

Increasing realness:

```
   tier 0:  inlay hint                        ghost text inline
            ┌──────────────────────────┐
            │ let x = 1; ▸ would write │      shown by editor, not in file
            │            ▸ "x = 2"     │      LSP method: textDocument/inlayHint
            └──────────────────────────┘      cheap, transient, view-only

   tier 1:  code lens / annotation            actionable preview
            ┌──────────────────────────┐
            │ let x = 1;               │
            │ ▶ Apply 3 staged writes  │      LSP: textDocument/codeLens
            └──────────────────────────┘            + workspace/executeCommand

   tier 2:  code action with WorkspaceEdit    actual edit, undoable
            ┌──────────────────────────┐
            │ Quick fix:               │      LSP: textDocument/codeAction
            │   Apply staged writes    │            → workspace/applyEdit
            └──────────────────────────┘      single button → real edit
                                              LSP-managed undo stack
```

Mapping to layers:

```
   StagingBatcher buffer   ──►  inlay hints  (transient, every keystroke)
                                code lens    ("you have N staged")
                                code action  ("apply staged now")
                                                     │
                                                     ▼
                                            client sends back
                                            workspace/applyEdit
                                                     │
                                                     ▼
                                       FsWriteBatcher actually runs
```

## Journal layer (rewind / forward)

Pure effects make replay trivial. Log = source of truth. Current state = `fold(replay, log)`. Same shape as redux DevTools.

```
   Effect log (append-only, persistent)
   ─────────────────────────────────────
   [E0] WriteEffect { path, range, bytes }
   [E1] WriteEffect { path, range, bytes }   ← head, current
   [E2] WriteEffect { path, range, bytes }
   [E3] WriteEffect { path, range, bytes }

   cursor at [E1] = "world is at this state"

   rewind to [E0]                     forward to [E3]
       │                                    │
       ▼                                    ▼
   re-run interpreter            re-run interpreter
   with empty initial            with prefix [E0..=E3]
   state, replay [E0]
```

Composes with effect_runtime:

```
   RtCtxBuilder
     .register(WriteEffect, JournaledBatcher::wrap(StagingBatcher))
                            │             ▲
                            │             │
                every call appended       wraps the
                to journal before         "real" interpreter
                forwarding to inner

   journal: Vec<(EffectId, BoxedEffect, Response)>

   rewind(t):
     1. clear inner state (StagingBatcher's Vec<WriteEdit>)
     2. re-run journal[..t] through inner
     3. expose new state to LSP
```

For pure effects, journal need not store responses; replay regenerates them. For impure (real reads of mutable FS), it must — those become "fixtures" on replay.

## Concrete additions to v3

1. `JournalLayer<E>` wraps any Batcher, appends `(EffectId, E, Response)` per call. ~80 LoC.
2. `StagingBatcher` for `WriteEffect` buffers into a `Store`. LSP reads the store. ~100 LoC.
3. LSP backend gains an `inlayHint` handler that reads the staging store.
4. LSP backend gains a `codeAction` handler that returns `WorkspaceEdit` constructed from the staging buffer.

The pattern: **pure effects let you swap policy without touching call sites**. Memoization is one policy, journaling is another, staging is another, dry-run is another. They compose.
