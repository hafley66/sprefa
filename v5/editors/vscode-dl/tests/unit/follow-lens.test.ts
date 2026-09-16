// Unit coverage for the "follow the user" navigation surface (Track B B4):
// the breadcrumb ring (`pushBreadcrumb`, inline in media/flow-panel.html) and
// the dl/locate debounce's stale-response guard (`isStaleSeq`, src/followLens.ts).
//
// `pushBreadcrumb` lives inline in flow-panel.html (the panel stays one
// self-contained file — no cross-file import under the webview CSP, per the
// portability law in extension.ts's header comment), so this test extracts it
// by brace-matching, the same technique scripts/test-panel-rollup.mjs uses for
// `applyCollapse` (see tests/README.md's "Follow-up: extracting the panel
// script" note — this is that pattern applied via vitest instead of a
// standalone node script).
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { isStaleSeq } from "../../src/followLens";

const here = dirname(fileURLToPath(import.meta.url));
const panelSource = readFileSync(join(here, "..", "..", "media", "flow-panel.html"), "utf8");

// Pull a `function NAME(...) { ... }` out of the source by matching braces
// from the first `{` after the signature (mirrors scripts/test-panel-rollup.mjs's
// extractFn).
function extractFn(source: string, name: string): string {
  const at = source.indexOf(`function ${name}(`);
  if (at < 0) throw new Error(`function not found: ${name}`);
  const open = source.indexOf("{", at);
  let depth = 0;
  for (let i = open; i < source.length; i++) {
    const c = source[i];
    if (c === "{") depth++;
    else if (c === "}") {
      depth--;
      if (depth === 0) return source.slice(at, i + 1);
    }
  }
  throw new Error(`unbalanced braces for ${name}`);
}

const pushBreadcrumb: (trail: string[], sym: string, max?: number) => string[] =
  // eslint-disable-next-line @typescript-eslint/no-implied-eval, no-new-func
  new Function(`${extractFn(panelSource, "pushBreadcrumb")}\nreturn pushBreadcrumb;`)();

describe("pushBreadcrumb (breadcrumb ring)", () => {
  it("pushes a new sym to the front", () => {
    expect(pushBreadcrumb([], "a")).toEqual(["a"]);
    expect(pushBreadcrumb(["a"], "b")).toEqual(["b", "a"]);
  });

  it("is most-recent-first: the latest push is always index 0", () => {
    const trail = pushBreadcrumb(pushBreadcrumb(pushBreadcrumb([], "a"), "b"), "c");
    expect(trail).toEqual(["c", "b", "a"]);
  });

  it("dedups: re-pushing an existing sym moves it to front instead of duplicating", () => {
    const trail = pushBreadcrumb(["c", "b", "a"], "b");
    expect(trail).toEqual(["b", "c", "a"]);
    expect(trail.filter((s) => s === "b")).toHaveLength(1);
  });

  it("caps at 8, dropping the oldest", () => {
    let trail: string[] = [];
    for (const sym of ["a", "b", "c", "d", "e", "f", "g", "h", "i"]) {
      trail = pushBreadcrumb(trail, sym, 8);
    }
    expect(trail).toHaveLength(8);
    expect(trail).toEqual(["i", "h", "g", "f", "e", "d", "c", "b"]);
    expect(trail).not.toContain("a"); // the oldest, pushed out at the 9th entry
  });

  it("defaults to a cap of 8 when max is omitted", () => {
    let trail: string[] = [];
    for (let i = 0; i < 10; i++) trail = pushBreadcrumb(trail, `sym${i}`);
    expect(trail).toHaveLength(8);
  });
});

describe("isStaleSeq (dl/locate debounce guard)", () => {
  it("is NOT stale when the sequence still matches the latest", () => {
    expect(isStaleSeq(3, 3)).toBe(false);
  });

  it("is stale when a later request has since been dispatched", () => {
    expect(isStaleSeq(2, 3)).toBe(true);
  });

  it("treats any mismatch as stale, not just 'behind'", () => {
    // A response for a request that was somehow ahead of the tracked counter
    // is equally untrustworthy — the guard is a strict equality check, not
    // an ordering check, so this drops too.
    expect(isStaleSeq(4, 3)).toBe(true);
  });
});
