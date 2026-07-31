/**
 * watchGlobDialect.test.ts — ONE glob dialect on BOTH halves of `bind watch`.
 *
 * Ruling `glob_dialect = node_matcher_both_halves` (rulings.pl, user
 * 2026-07-31): the boot half and the live half both decide membership with
 * node's `path.matchesGlob`. Boot enumerates the whole tracked worktree
 * (`git ls-files` with NO pathspec) and filters in JS with the same call the
 * live half makes; git pathspec leaves the glob path entirely.
 *
 * WHY THIS FILE EXISTS. Before the ruling, boot selected with
 * `git ls-files -- <glob>` (pathspec) and live selected with `matchesGlob`.
 * The two dialects disagreed on 170 of the 242 `scan()` globs in the v5
 * corpus (census: plans/2026-07-31-scan-spelling-card.md §2), and the
 * disagreement was SILENT — a file the boot half dropped appeared on its first
 * edit and was deleted again on the next restart, because boot reconciles
 * durable rows against its own answer. A flickering row, no error anywhere.
 *
 * FAIL-FIRST RECEIPTS, measured on this tree at base 69fbdb50 with the boot
 * half still on pathspec. Each of the first three tests below is RED there and
 * green after, and each pins one of the three shapes the census measured:
 *
 *   1. `src/**\/*.rs`   pathspec `**` requires at least one directory, so every
 *                       DIRECT child of `src/` was dropped. Against this repo:
 *                       `git ls-files -- 'src/**\/*.rs'` = 145 rows, 0 of them
 *                       direct children, while 55 direct children are tracked.
 *                       RED as: expected src/lib.rs in the boot batch.
 *   2. `**\/*.md`       same rule at the root: 837 rows, 0 root-level, while 9
 *                       root-level .md files are tracked. This is
 *                       GETTING-STARTED.md's own tutorial glob (line 159), so
 *                       the page taught a shape that silently lost files.
 *                       RED as: expected README.md in the boot batch.
 *   3. `*.{rs,ts}`      brace alternation is not pathspec syntax at all;
 *                       `git ls-files -- '*.{rs,ts}'` returns ZERO rows for the
 *                       whole glob. MEASURED on node v24.15.0 before choosing
 *                       the assertion: `path.matchesGlob('a.rs', '*.{rs,ts}')`
 *                       is true, so braces need no named refusal here — they
 *                       simply work once the matcher is the matcher. The test
 *                       asserts ROWS, never a refusal.
 *
 * The fourth test is the standing property the ruling actually buys: for one
 * fixed tree and the five glob shapes the census bucketed the corpus into, the
 * set of paths BOOT accepts equals the set of paths LIVE accepts. It compares
 * the two halves through the real `WatchBindRunner` rather than by calling the
 * matcher twice, so a future change that reintroduces a second dialect on
 * either side fails here even if the matcher itself is untouched.
 *
 * The live leg runs against a NON-repository copy of the same tree. That is
 * not a workaround: `trackedPaths` is empty outside a repo, so boot emits
 * nothing and every row in that leg came through `batchFor` — which is exactly
 * the isolation the property needs.
 *
 * SABOTAGE RECEIPTS (run 2026-07-31, both reverted; tree clean after):
 *
 *   1. the pre-fix boot half itself — `trackedPaths(root, glob)` selecting by
 *      pathspec — is RED 4/4, quoted per shape in the commit that added this
 *      file before the fix landed.
 *   2. widening the shared matcher to `true || path.matchesGlob(...)` is also
 *      RED 4/4, and test 4 fails with `'src/**\/*.rs' hashed a decoy file, so
 *      the filter ran after the digest`. That is worth stating precisely,
 *      because it is NOT the boot-vs-live comparison that catches it: a matcher
 *      that accepts everything makes both halves agree, and the equality
 *      assertion passes. The decoy files are what make test 4 discriminating in
 *      that direction, which is why they are in the tree.
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";

import { Observable, Subject, VirtualTimeScheduler, of } from "rxjs";

import { WatchBindRunner } from "../serve/2_binds.ts";
import type {
  IArrivalBatch,
  IBindPlan,
  ILiveEngine,
  IRow,
  ITickOutcome,
  IWatchSource,
} from "../runtime/types.ts";

const COALESCE_MS = 50;

/** The tree every test in this file uses. Chosen so each of the census
 *  buckets has at least one file on BOTH sides of its boundary: a direct child
 *  and a nested file under `src/`, a root-level and a nested `.md`, and two
 *  extensions for the brace shape. `noise/*.txt` are decoys that no glob here
 *  matches, so a leg that hashes them shows up as an extra row. */
const TREE: readonly (readonly [string, string])[] = [
  ["src/lib.rs", "direct child of src\n"],
  ["src/nested/deep.rs", "nested under src\n"],
  ["src/nested/deeper/deepest.rs", "two levels under src\n"],
  ["README.md", "root level markdown\n"],
  ["docs/guide.md", "nested markdown\n"],
  ["top.ts", "root level typescript\n"],
  ["noise/a.txt", "decoy\n"],
  ["noise/b.txt", "decoy\n"],
];

function git(root: string, ...arguments_: readonly string[]): void {
  const result = spawnSync("git", arguments_, { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
}

/** Writes TREE into `root`. Tracks it in a fresh repo unless `track` is false,
 *  which is how the live leg gets an empty `git ls-files` answer. */
function plantTree(root: string, track = true): void {
  for (const [relativePath, body] of TREE) {
    mkdirSync(join(root, dirname(relativePath)), { recursive: true });
    writeFileSync(join(root, relativePath), body);
  }
  if (!track) return;
  git(root, "init", "-q");
  git(root, "add", "--", ...TREE.map(([relativePath]) => relativePath));
}

class ScriptedWatchSource implements IWatchSource {
  private readonly paths = new Subject<string>();

  constructor(private readonly scheduler: VirtualTimeScheduler) {}

  watch(): Observable<string> {
    return this.paths.asObservable();
  }

  notify(...paths: readonly string[]): void {
    for (const path of paths) this.paths.next(path);
  }

  settle(): void {
    this.scheduler.maxFrames = this.scheduler.frame + COALESCE_MS * 2;
    this.scheduler.flush();
  }
}

function watchPlan(glob: string): IBindPlan {
  return {
    name: "watch",
    columns: [
      { name: "glob", type: "text" },
      { name: "path", type: "text" },
      { name: "digest", type: "text" },
    ],
    literals: [glob],
    execution: "live_watch",
  };
}

function collectingEngine(storedRows: readonly IRow[]): {
  readonly engine: ILiveEngine;
  readonly batches: IArrivalBatch[];
} {
  const batches: IArrivalBatch[] = [];
  let tick = 0;
  const engine = {
    rows(): Observable<readonly IRow[]> {
      return of(storedRows);
    },
    submit(batch: IArrivalBatch): Observable<ITickOutcome> {
      batches.push(batch);
      tick += 1;
      return of({ tick, line: "", deltas: { rels: [], carryPending: false } });
    },
  } as unknown as ILiveEngine;
  return { engine, batches };
}

/** Paths the BOOT half accepts for `glob`: a fresh runner over a tracked tree
 *  with no durable rows emits one `add` per accepted path. */
function bootAccepts(glob: string): readonly string[] {
  const root = mkdtempSync(join(tmpdir(), "tsv2-glob-boot-"));
  try {
    plantTree(root);
    const scheduler = new VirtualTimeScheduler();
    const collected = collectingEngine([]);
    const running = new WatchBindRunner(collected.engine, [watchPlan(glob)], {
      root,
      coalesceMs: COALESCE_MS,
      scheduler,
      source: new ScriptedWatchSource(scheduler),
    }).firings$.subscribe();
    running.unsubscribe();
    return (collected.batches[0] ?? []).map((arrival) => String(arrival.row[1]));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

/** Paths the LIVE half accepts for `glob`: the same tree UNTRACKED, so boot
 *  contributes nothing, then every file notified through the watch source. */
function liveAccepts(glob: string): readonly string[] {
  const root = mkdtempSync(join(tmpdir(), "tsv2-glob-live-"));
  try {
    plantTree(root, false);
    const scheduler = new VirtualTimeScheduler();
    const source = new ScriptedWatchSource(scheduler);
    const collected = collectingEngine([]);
    const running = new WatchBindRunner(collected.engine, [watchPlan(glob)], {
      root,
      coalesceMs: COALESCE_MS,
      scheduler,
      source,
    }).firings$.subscribe();
    source.notify(...TREE.map(([relativePath]) => join(root, relativePath)));
    source.settle();
    running.unsubscribe();
    return collected.batches.flatMap((batch) => batch.map((arrival) => String(arrival.row[1])));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

const sorted = (paths: readonly string[]): readonly string[] => [...paths].sort();

test("watch boot: `src/**/*.rs` admits direct children of src/, not only nested ones", () => {
  const booted = sorted(bootAccepts("src/**/*.rs"));
  assert.deepEqual(booted, ["src/lib.rs", "src/nested/deep.rs", "src/nested/deeper/deepest.rs"]);
});

test("watch boot: `**/*.md` admits repo-root files", () => {
  const booted = sorted(bootAccepts("**/*.md"));
  assert.deepEqual(booted, ["README.md", "docs/guide.md"]);
});

test("watch boot: a brace glob boots rows rather than a silent zero", () => {
  // Braces are matcher syntax, not pathspec syntax. `*` still does not cross
  // `/`, so src/lib.rs is correctly absent and top.ts is correctly present.
  const booted = sorted(bootAccepts("*.{rs,ts}"));
  assert.deepEqual(booted, ["top.ts"]);
});

test("watch: boot and live accept identical path sets across every census glob shape", () => {
  // One shape per bucket of plans/2026-07-31-scan-spelling-card.md §0, in the
  // order that table lists them.
  const shapes = [
    "src/**/*.rs", // A: dir + recursive, 112 corpus sites
    "README.md", // B: exact path, 69 sites
    "src/*.rs", // C: single `*`, 30 sites
    "*.{rs,ts}", // D: brace alternation, 18 sites
    "**/*.md", // E: leading `**/`, 18 sites
  ];
  for (const glob of shapes) {
    const boot = sorted(bootAccepts(glob));
    const live = sorted(liveAccepts(glob));
    assert.deepEqual(boot, live, `boot and live disagree on '${glob}'`);
    assert.ok(boot.length > 0, `'${glob}' selected nothing, so the comparison is vacuous`);
    assert.ok(
      boot.every((relativePath) => !relativePath.startsWith("noise/")),
      `'${glob}' hashed a decoy file, so the filter ran after the digest`,
    );
  }
});
