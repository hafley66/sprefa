import type {
  IRelCatalogRow,
  IReloadPlan,
  IReloadPlanner,
  RelVerdict,
} from "../runtime/types.ts";

// module_id is positional across emits (one id sequence with primitives and
// synthetic rows), so keying on it reads any id-layout shift as drop-everything.
function rel_key(row: IRelCatalogRow, by_id: ReadonlyMap<number, IRelCatalogRow>): string {
  const parts: string[] = [row.local_name];
  let parent = by_id.get(row.parent_id);
  while (parent !== undefined && parent.kind === "rel") {
    parts.push(parent.local_name);
    parent = by_id.get(parent.parent_id);
  }
  return parts.reverse().join(":");
}

function rels_by_key(rows: readonly IRelCatalogRow[]): Map<string, IRelCatalogRow> {
  const by_id = new Map<number, IRelCatalogRow>();
  for (const row of rows) by_id.set(row.rel_id, row);
  const rels = new Map<string, IRelCatalogRow>();
  for (const row of rows) {
    if (row.kind === "rel") rels.set(rel_key(row, by_id), row);
  }
  return rels;
}

export const ReloadPlanner: IReloadPlanner = {
  plan(prev, next, allow_drop) {
    const prev_rels = rels_by_key(prev);
    const next_rels = rels_by_key(next);

    const verdicts = new Map<string, RelVerdict>();
    const statements: string[] = [];
    const unsupported: string[] = [];

    for (const [key, next_row] of next_rels) {
      const prev_row = prev_rels.get(key);
      if (prev_row === undefined) {
        verdicts.set(key, "create");
      } else if (prev_row.h_schema !== next_row.h_schema) {
        verdicts.set(key, "recreate");
        statements.push(`DROP TABLE IF EXISTS "${next_row.local_name}"`);
      } else if (prev_row.h_rule !== next_row.h_rule) {
        verdicts.set(key, "refill");
        statements.push(`DELETE FROM "${next_row.local_name}"`);
      } else {
        verdicts.set(key, "keep");
      }
    }

    for (const [key, prev_row] of prev_rels) {
      if (next_rels.has(key)) continue;
      if (allow_drop) {
        verdicts.set(key, "drop");
        statements.push(`DROP TABLE IF EXISTS "${prev_row.local_name}"`);
      } else {
        unsupported.push(`rel_drop_needs_allow_drop(${prev_row.local_name})`);
      }
    }

    return { verdicts, statements, unsupported };
  },
};
