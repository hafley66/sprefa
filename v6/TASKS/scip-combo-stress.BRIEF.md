# Lane brief: diet-SCIP combo stress — sprefa-extract × soopy from authored dl6

First action: `git merge --ff-only cf6c4259`. Failure = STOP AND REPORT.

## Goal

Stress the combinations of the hosted soopy and sprefa-extract surfaces from
authored dl6 programs, differentially judged. The point is to find combos that
compile but answer wrong, or refuse for no cited reason.

## The hosted surface you are stressing (all on main at cf6c4259)

- soopy file arms (`v6/sprefa-engine-rs/src/hosts.rs:126-127`): `files`,
  `files_at`, `repo_files`, `repo_files_at` (worktree + named-rev).
- extraction arms (`hosts.rs:36-37`): `sprefa_extract`, `sprefa_extract_repo`
  (in-process, all families: defs/refs/calls/df spans — the diet SCIP).
- dep crawl arms (landed PR #289): `dep_crawl_repo/visited/edge/unresolved`.
- Templates to copy: `v6/tsv2/goldens/multirepo_crawl/0_multirepo_crawl.dl6`,
  `4_dep_crawl.dl6`, gates `2_gate.sh`, `5_dep_gate.sh`.

## Deliverable

New dir `v6/tsv2/goldens/scip_combo/` holding numbered dl6 programs + one
gate script + a README table. Combos to spell, minimum set:

1. `repo_files_at` at rev A joined to `repo_files_at` at rev B (same repo,
   pinned corpus revs): added/removed file rows by set difference.
2. `sprefa_extract_repo` over worktree vs over a pinned rev's file set:
   extraction facts joined on file identity, diffed.
3. Extraction def/ref spans joined against the file rows they came from
   (span-in-file containment; every span's file must exist in `files`).
4. dep_crawl edges × per-repo extraction: for each visited repo, extract and
   count facts per family; a repo visited by the crawl with zero extractable
   files is a named row, never silence.
5. Cross-repo ref chase: defs in repo X whose name appears as a ref in repo Y
   (name-level join only; no new resolution machinery).
6. Same program judged on BOTH doors where the construct set allows: TS
   runtime and Rust runtime, byte-diff the rel dumps (the 2_gate.sh pattern).

## Judging

- Every program's rel dumps byte-diffed TS-door vs Rust-door where both run;
  where only one door runs, cite why (manifest bucket).
- A combo that fails to compile: cite `v6/prolog/compile/out/manifest.json`
  bucket + reason; it goes in the README failure table, NOT silently dropped.
- A combo that compiles and disagrees between doors: that is the prize.
  Shrink it to the minimal program, pin it as its own numbered file, name it
  loudly in the README and your final report.
- Corpus: the pinned multirepo corpus (`1_corpus.sh` pattern). No network.

## Receipts (three runs each)

```bash
cd v6 && just scip-combo   # the leg you add to v6/justfile
cd v6 && just multirepo-golden   # must stay green, you share the corpus
```

Commit with `COMMENT_RAIL_IDLE_MS=3000 git commit ...`, never pipe a commit,
check `git log` before finishing.

## File ownership

OWNS: `v6/tsv2/goldens/scip_combo/**`, `v6/justfile` (one leg block only).

FORBIDDEN: everything under `v6/sprefa-engine-rs/src/` and
`v6/sprefa-extract/src/` and `v6/prolog/`. If a combo needs an engine or
compiler change, WRITE IT in the README failure table with the file:line it
blocks on, and stop that combo. Another lane owns hosts.rs tonight.

## Laws

- dl variable names descriptive, never single-letter, in every program.
- Language vocabulary: rxjs/prolog/SQL words only.
- Comment budget: constraints only.
- A permission denial ends the approach; report, never work around.
