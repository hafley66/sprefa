// Unit coverage for the "exploded" list mode (Track C, C3): tier-major
// stratum reorder + band-row synthesis (`explodeRows`, inline in
// media/flow-panel.html). Same brace-matching extraction technique
// scripts/test-panel-rollup.mjs and tests/unit/follow-lens.test.ts use --
// explodeRows calls buildRows internally (one path-trie run per stratum, per
// its own header comment), which in turn calls siblingCmp, which reads the
// module-level `bomSort` state -- all three are extracted together and
// `bomSort` is declared by hand in the assembled scope.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const panelSource = readFileSync(join(here, "..", "..", "media", "flow-panel.html"), "utf8");

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

type Row = Record<string, unknown>;
type ExplodeResult = { rows: Row[]; singleStratum: boolean };

const explodeRows: (nodeRows: unknown[][]) => ExplodeResult = (() => {
  const source =
    "let bomSort = 'line';\n" +
    extractFn(panelSource, "siblingCmp") + "\n" +
    extractFn(panelSource, "buildRows") + "\n" +
    extractFn(panelSource, "explodeRows") + "\n" +
    "return explodeRows;";
  // eslint-disable-next-line @typescript-eslint/no-implied-eval, no-new-func
  return new Function(source)();
})();

// Positional shape (PRESETS.bomTable's query, see flow-panel.html):
//   id, label, kind, file, line, parent,
//   member_count, fan_in, fan_out, weight, tier, sccRep, sccSize
const row = (
  id: string, file: string, tier: number, rep: string | null = null, size = 1
): unknown[] => [id, id, "fn", file, 1, "", 0, 0, 0, 0, tier, rep, size];

describe("explodeRows (tier-major stratum reorder + band synthesis)", () => {
  it("groups three strata tier-ascending, each with its own band row", () => {
    const nodeRows = [
      row("c::fn", "c.rs", 0),
      row("b::fn", "b.rs", 1),
      row("a::fn", "a.rs", 2),
    ];
    const { rows, singleStratum } = explodeRows(nodeRows);
    expect(singleStratum).toBe(false);
    const bands = rows.filter((r) => r.type === "band");
    expect(bands.map((b) => b.tier)).toEqual([0, 1, 2]);
    expect(bands.every((b) => b.fileCount === 1)).toBe(true);
    expect(bands.every((b) => b.single === false)).toBe(true);
    // tier-major: every row inside tier 0's block precedes tier 1's block,
    // which precedes tier 2's -- assert via index of each file's node row.
    const idxOf = (id: string) => rows.findIndex((r) => r.id === id);
    expect(idxOf("c::fn")).toBeLessThan(idxOf("b::fn"));
    expect(idxOf("b::fn")).toBeLessThan(idxOf("a::fn"));
  });

  it("keeps the existing path-trie order WITHIN a stratum", () => {
    // two files sharing tier 1, both under src/ -- buildRows compacts them
    // under one "src" dir row; explodeRows must preserve that shape inside
    // the tier-1 band, not flatten it.
    const nodeRows = [
      row("z::fn", "src/z.rs", 1),
      row("a::fn", "src/a.rs", 1),
    ];
    const { rows } = explodeRows(nodeRows);
    const band = rows.find((r) => r.type === "band" && r.tier === 1);
    expect(band).toBeTruthy();
    const dirRow = rows.find((r) => r.type === "dir");
    expect(dirRow?.label).toBe("src");
    // alphabetical file order inside the compacted dir (buildRows' own trie
    // walk sorts children by segment name), same as the non-exploded path.
    const idxOf = (id: string) => rows.findIndex((r) => r.id === id);
    expect(idxOf("a::fn")).toBeLessThan(idxOf("z::fn"));
  });

  it("welds a >1-member cluster into one collapsible card, members nested under it", () => {
    const nodeRows = [
      row("x::fn", "x.rs", 0, "x.rs", 2),
      row("y::fn", "y.rs", 0, "x.rs", 2),
    ];
    const { rows } = explodeRows(nodeRows);
    const weldCard = rows.find((r) => r.kind === "weld");
    expect(weldCard).toBeTruthy();
    expect(weldCard?.hasChildren).toBe(true);
    expect(String(weldCard?.label)).toContain("2 files");
    const weldIdx = rows.indexOf(weldCard!);
    const xIdx = rows.findIndex((r) => r.id === "x::fn");
    const yIdx = rows.findIndex((r) => r.id === "y::fn");
    // members sit AFTER the weld card and are indented deeper (nested).
    expect(xIdx).toBeGreaterThan(weldIdx);
    expect(yIdx).toBeGreaterThan(weldIdx);
    expect((rows[xIdx].depth as number)).toBeGreaterThan(weldCard!.depth as number);
    // exactly one weld card for the pair, not one per member.
    expect(rows.filter((r) => r.kind === "weld")).toHaveLength(1);
  });

  it("keeps two UNRELATED clusters separate (bug dag-layers' global-min shape had)", () => {
    const nodeRows = [
      row("m::fn", "m.rs", 0, "m.rs", 2),
      row("n::fn", "n.rs", 0, "m.rs", 2),
      row("x::fn", "x.rs", 0, "x.rs", 2),
      row("y::fn", "y.rs", 0, "x.rs", 2),
    ];
    const { rows } = explodeRows(nodeRows);
    const weldCards = rows.filter((r) => r.kind === "weld");
    expect(weldCards).toHaveLength(2); // NOT one giant 4-file weld
    for (const card of weldCards) expect(String(card.label)).toContain("2 files");
  });

  it("a rep with only ONE surviving file in this slice renders plain, not welded", () => {
    // sccRep/sccSize say the cluster has 2 members, but only one of them
    // is actually present in this node-row set (e.g. dropped by linkedOnly
    // upstream in renderList) -- must not render a 1-member "weld".
    const nodeRows = [row("x::fn", "x.rs", 0, "x.rs", 2)];
    const { rows } = explodeRows(nodeRows);
    expect(rows.some((r) => r.kind === "weld")).toBe(false);
    expect(rows.some((r) => r.id === "x::fn")).toBe(true);
  });

  it("degenerate: rows with no tier column collapse to one 'no layering' band", () => {
    const nodeRows = [
      ["a::fn", "a::fn", "fn", "a.rs", 1, ""],
      ["b::fn", "b::fn", "fn", "b.rs", 1, ""],
    ];
    const { rows, singleStratum } = explodeRows(nodeRows);
    expect(singleStratum).toBe(true);
    const bands = rows.filter((r) => r.type === "band");
    expect(bands).toHaveLength(1);
    expect(bands[0].single).toBe(true);
    expect(bands[0].fileCount).toBe(2);
  });

  it("empty input: no rows, no band, no crash", () => {
    const { rows, singleStratum } = explodeRows([]);
    expect(rows).toEqual([]);
    expect(singleStratum).toBe(true);
  });
});
