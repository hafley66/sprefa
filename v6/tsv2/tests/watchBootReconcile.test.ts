/**
 * watchBootReconcile.test.ts — one tracked-worktree reconcile per watched glob
 * at subscribe, followed by the unchanged live path.
 *
 * The engine and notification source are injected. The filesystem and Git
 * index are real, so `git ls-files` pathspec selection and byte digests cross
 * the same seam as serving.
 *
 * FAIL-FIRST is recorded end to end in scripts/extraction-live.sh. Before the
 * reconcile landed, deleting tracked src/d.ts while the server was down
 * produced:
 *   FAIL  phase 7: watch row survived deletion while server was down
 */

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
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

const GLOB = "**/*.ts";
const COALESCE_MS = 50;

function digest(text: string): string {
  return bytesToHex(sha256(Buffer.from(text)));
}

function git(root: string, ...arguments_: readonly string[]): void {
  const result = spawnSync("git", arguments_, { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
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

function watchPlan(): IBindPlan {
  return {
    name: "watch",
    columns: [
      { name: "glob", type: "text" },
      { name: "path", type: "text" },
      { name: "digest", type: "text" },
    ],
    literals: [GLOB],
    execution: "live_watch",
  };
}

function recordingEngine(storedRows: readonly IRow[]): {
  readonly engine: ILiveEngine;
  readonly rowReads: string[];
  readonly batches: IArrivalBatch[];
} {
  const rowReads: string[] = [];
  const batches: IArrivalBatch[] = [];
  let tick = 0;
  const engine = {
    rows(rel: string): Observable<readonly IRow[]> {
      rowReads.push(rel);
      return of(storedRows);
    },
    submit(batch: IArrivalBatch): Observable<ITickOutcome> {
      batches.push(batch);
      tick += 1;
      return of({ tick, line: "", deltas: { rels: [], carryPending: false } });
    },
  } as unknown as ILiveEngine;
  return { engine, rowReads, batches };
}

test("watch boot: stored rows and tracked disk become one difference batch, then seed a later delete", () => {
  const root = mkdtempSync(join(tmpdir(), "tsv2-watch-boot-"));
  const scheduler = new VirtualTimeScheduler();
  const source = new ScriptedWatchSource(scheduler);
  try {
    git(root, "init", "-q");
    mkdirSync(join(root, "src"));
    writeFileSync(join(root, "src/added.ts"), "added\n");
    writeFileSync(join(root, "src/changed.ts"), "new\n");
    writeFileSync(join(root, "src/missing.ts"), "missing\n");
    writeFileSync(join(root, "src/unchanged.ts"), "same\n");
    git(root, "add", "--", "src/added.ts", "src/changed.ts", "src/missing.ts", "src/unchanged.ts");
    rmSync(join(root, "src/missing.ts"));

    const recorded = recordingEngine([
      [GLOB, "src/changed.ts", digest("old\n")],
      [GLOB, "src/missing.ts", digest("missing\n")],
      [GLOB, "src/unchanged.ts", digest("same\n")],
      ["other/**/*.ts", "ignored.ts", digest("ignored\n")],
    ]);
    const firings: unknown[] = [];
    const running = new WatchBindRunner(recorded.engine, [watchPlan()], {
      root,
      coalesceMs: COALESCE_MS,
      scheduler,
      source,
    }).firings$.subscribe((firing) => firings.push(firing));

    assert.deepEqual(recorded.rowReads, ["watch"], "one engine-state read is owed for this watched glob");
    assert.deepEqual(recorded.batches, [
      [
        { rel: "watch", sign: "del", row: [GLOB, "src/changed.ts", digest("old\n")] },
        { rel: "watch", sign: "add", row: [GLOB, "src/changed.ts", digest("new\n")] },
        { rel: "watch", sign: "del", row: [GLOB, "src/missing.ts", digest("missing\n")] },
        { rel: "watch", sign: "add", row: [GLOB, "src/added.ts", digest("added\n")] },
      ],
    ]);
    assert.deepEqual(firings, [{ rel: "watch", glob: GLOB, added: 2, removed: 2, tick: 1 }]);

    rmSync(join(root, "src/unchanged.ts"));
    source.notify(join(root, "src/unchanged.ts"));
    source.settle();

    assert.deepEqual(recorded.batches[1], [
      { rel: "watch", sign: "del", row: [GLOB, "src/unchanged.ts", digest("same\n")] },
    ]);
    assert.deepEqual(firings[1], { rel: "watch", glob: GLOB, added: 0, removed: 1, tick: 2 });
    running.unsubscribe();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("watch boot: identical tracked rows seed state without submitting a tick", () => {
  const root = mkdtempSync(join(tmpdir(), "tsv2-watch-boot-zero-"));
  const scheduler = new VirtualTimeScheduler();
  const source = new ScriptedWatchSource(scheduler);
  try {
    git(root, "init", "-q");
    mkdirSync(join(root, "src"));
    writeFileSync(join(root, "src/stable.ts"), "stable\n");
    git(root, "add", "--", "src/stable.ts");

    const recorded = recordingEngine([[GLOB, "src/stable.ts", digest("stable\n")]]);
    const running = new WatchBindRunner(recorded.engine, [watchPlan()], {
      root,
      coalesceMs: COALESCE_MS,
      scheduler,
      source,
    }).firings$.subscribe();

    assert.deepEqual(recorded.rowReads, ["watch"]);
    assert.deepEqual(recorded.batches, [], "an identical restart owes zero boot ticks");

    source.notify(join(root, "src/stable.ts"));
    source.settle();
    assert.deepEqual(recorded.batches, [], "an unchanged post-boot notification also owes zero ticks");
    running.unsubscribe();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("watch boot: notifications arriving during the row read queue behind the boot batch", () => {
  const root = mkdtempSync(join(tmpdir(), "tsv2-watch-boot-queue-"));
  const scheduler = new VirtualTimeScheduler();
  const source = new ScriptedWatchSource(scheduler);
  try {
    git(root, "init", "-q");
    mkdirSync(join(root, "src"));
    writeFileSync(join(root, "src/early.ts"), "early\n");

    const storedRows = new Subject<readonly IRow[]>();
    const batches: IArrivalBatch[] = [];
    let tick = 0;
    const engine = {
      rows(): Observable<readonly IRow[]> {
        return storedRows;
      },
      submit(batch: IArrivalBatch): Observable<ITickOutcome> {
        batches.push(batch);
        tick += 1;
        return of({ tick, line: "", deltas: { rels: [], carryPending: false } });
      },
    } as unknown as ILiveEngine;
    const running = new WatchBindRunner(engine, [watchPlan()], {
      root,
      coalesceMs: COALESCE_MS,
      scheduler,
      source,
    }).firings$.subscribe();

    source.notify(join(root, "src/early.ts"));
    source.settle();
    assert.deepEqual(batches, [], "live windows do not cross the seam before boot reconciliation");

    storedRows.next([]);
    storedRows.complete();
    assert.deepEqual(batches, [
      [{ rel: "watch", sign: "add", row: [GLOB, "src/early.ts", digest("early\n")] }],
    ]);
    running.unsubscribe();
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
