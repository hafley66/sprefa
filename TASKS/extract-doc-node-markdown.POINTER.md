# Lane brief: markdown `doc_node` plus the `doc_ref` bridge

First action: `git merge --ff-only 6b483939bcfabb329f0cc424ce85aec709acadc3`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

Second action: READ YOUR FULL BRIEF, at this absolute path, and follow it exactly:

    /Users/chrishafley/projects/sprefa/TASKS/extract-doc-node-markdown.BRIEF.md

It is outside your worktree on purpose. Read it whole before you edit anything.
It carries the pinned design, the file ownership list, the forbidden files, the
test spec, and the gate. If you cannot read it, STOP AND REPORT rather than
guessing the task.

Everything below is a summary; the file above wins wherever they differ.

- Goal: markdown gets a type plane. `DocNode`/`DocNodeKind` on `TypeFAux`, a
  heading-stack walker feeding BOTH `cst` and `types` from ONE parse in
  `src/lang/markdown/_0_source.rs`, a `doc_node` wire arm plus schema record,
  `TypeEdgeKind::DocRef` plus `impl Resolve<TypeF> for MarkdownSource` bridging
  headings to the corpus `DefIndex`, the markdown `ResolveArm` types slot, and a
  new `tests/22_doc_node.rs`.
- Write the test FIRST and paste the red, then land the walker and paste green.
- Never spawn a subagent. Never `git commit -n`. Never edit `.githooks/**`.
- Bare `cargo fmt` is banned; only `cargo fmt -- <your owned files>`.
- `tests/golden_parity.rs` is FORBIDDEN: its match already ends in a catch-all,
  so a new `FlatFact` variant needs no edit there.
- NEVER add a new `.ts` file under `tests/fixtures/ts/`: the scip ratchet walks
  that root recursively and demands a scip occurrence per call site.
- Comment budget: the pre-commit rail FAILS any staged hunk with more than 2
  consecutive comment lines. Keep doc comments to 2 lines.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.
- Push, open a PR whose body ends with `Refs-Issue: extract-doc-node-markdown`,
  and NEVER merge it. Report the URL and stop.
