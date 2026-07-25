/**
 * The shared 53-bit content-digest fold used by rowDigest (2_schema.ts) and
 * effectDigest (1_hosts.ts). Imports only the store's content_digest, no
 * package-local import, so both callers can sit at their own numbered depths
 * without an upward-import violation.
 *
 * Mixes the row's column values (booleans normalized to 0/1, everything else
 * passed through) via content_digest, then narrows the signed 64-bit fold to
 * a 53-bit-safe JS number: asUintN(64) makes it unsigned first, asUintN(53)
 * narrows to what a JS number holds exactly.
 */
import { content_digest } from "sprefa-store-engine/src/engine/ingest.ts";

type DigestableValue = string | number | boolean | null;

function toDigestPart(value: DigestableValue): number | string | null {
  return typeof value === "boolean" ? (value ? 1 : 0) : value;
}

export function foldRowDigest(row: Record<string, DigestableValue>, columns: readonly string[]): number {
  const parts = columns.map((column) => toDigestPart(row[column] ?? null));
  const folded = content_digest(parts);
  return Number(BigInt.asUintN(53, BigInt.asUintN(64, folded)));
}
