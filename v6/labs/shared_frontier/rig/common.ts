/**
 * Shared helpers for the shared-frontier measurement lab.
 *
 * Driver is `@libsql/client` (the same one `v6/tsv2/runtime/scratchStore.ts`
 * opens through `open_db`), url `:memory:`, default intMode.
 */

import { createClient, type Client } from "@libsql/client";

export interface ITiming {
  readonly label: string;
  readonly ms: number;
}

export function openMemory(): Client {
  return createClient({ url: ":memory:" });
}

/** Median of a sample. Every number in the report is a median of >= 5 runs. */
export function median(values: readonly number[]): number {
  const sorted = [...values].sort((left, right) => left - right);
  const mid = sorted.length >> 1;
  return sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

export function round(value: number, digits: number): number {
  const scale = 10 ** digits;
  return Math.round(value * scale) / scale;
}

/** Markdown table from a header row plus body rows, all cells pre-stringified. */
export function markdownTable(header: readonly string[], rows: readonly (readonly string[])[]): string {
  const lines = [`| ${header.join(" | ")} |`, `| ${header.map(() => "---").join(" | ")} |`];
  for (const row of rows) lines.push(`| ${row.join(" | ")} |`);
  return lines.join("\n");
}

/**
 * The transient-table prefixes `lower.pl` mints per relation. Any table whose
 * name starts with one of these is per-relation transient state; everything
 * else is durable.
 */
export const TRANSIENT_PREFIXES: readonly string[] = [
  "__delta_",
  "__frontier_",
  "__next_frontier_",
  "__departure_frontier_",
  "__support_next_",
  "__pre_",
  "__new_",
  "__expand_a_",
  "__expand_b_",
  "__ping_",
  "__pong_",
  "__cone_",
  "__agg_scope_",
  "__avg_acc_",
];

export function isTransient(name: string): boolean {
  return TRANSIENT_PREFIXES.some((prefix) => name.startsWith(prefix));
}

export function transientFamily(name: string): string | null {
  return TRANSIENT_PREFIXES.find((prefix) => name.startsWith(prefix)) ?? null;
}
