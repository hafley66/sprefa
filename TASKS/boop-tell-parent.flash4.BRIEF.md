# Boop tell-parent

Repository: `/Users/chrishafley/projects/hafley-rs`

Issue: `boop-tell-parent` on the hafley-rs issuectl board.

Implement a least-argument parent message command:

```text
boop tell-parent --kind completion --body TEXT
```

Contract:

- Resolve the caller using the existing harness identity trait and registered route data.
- Resolve the caller's parent from the existing registered parent edge.
- Reuse the existing mail row and delivery/injection path used by `boop beep hail`.
- Print the created message ID so the caller can await or cite it.
- The caller supplies no lane name or parent route.
- Missing caller identity, missing parent edge, and ambiguous identity are named errors.
- Keep harness-specific identity discovery inside each harness implementation.
- Add deterministic tests for a lane and a registered pane-less/native agent.
- Add the command to `boop --help` doctrine.
- Do not implement parent broadcast in this card.

Before editing, inspect `boop --help`, the current hail path, route/parent storage,
caller identity resolution, and the associated tests. Reuse those seams.

Run CI covering the library and CLI parser/execution path. Commit with:

```text
Refs-Issue: @boop-tell-parent
```

On completion, use the new command itself if operational. Otherwise send:

```text
boop beep hail codex-147 --kind completion --body "boop-tell-parent done commit=<hash> CI=<result>"
```

Report the message ID.
