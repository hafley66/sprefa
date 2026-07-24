/**
 * 0_digest.ts — the shared 53-bit content-digest fold, used by rowDigest
 * (2_schema.ts) and effectDigest (1_hosts.ts). M7 consolidation: those two
 * files independently duplicated this fold (2_schema.ts's header used to say
 * "reused verbatim via content_digest (do not re-implement the fold)", and
 * 1_hosts.ts's header documented the duplication as deliberate, "same
 * content_digest import, same asUintN(53) narrowing, documented once here
 * rather than imported" — both files now import the one implementation below
 * instead).
 *
 * Runtime code, depth-0 (imports ONLY the store's content_digest — no
 * package-local import, not even 0_types.ts, so both 1_hosts.ts and
 * 2_schema.ts can sit at their own numbered depths and import this without an
 * upward-import violation).
 *
 * Law: mix the row's column values (booleans normalized to 0/1, everything
 * else passed through) via content_digest, then narrow the signed 64-bit fold
 * to a 53-bit-safe JS number: asUintN(64) makes it unsigned first, asUintN(53)
 * narrows to what a JS number holds exactly.
 */
import { content_digest } from "sprefa-store-engine/src/engine/ingest.ts";

type DigestableValue = string | number | boolean | null;

function toDigestPart(value: DigestableValue): number | string | null {
  return typeof value === "boolean" ? (value ? 1 : 0) : value;
}

/** Fold `row`'s named `columns` (in order) into a 53-bit-safe digest number. */
export function foldRowDigest(row: Record<string, DigestableValue>, columns: readonly string[]): number {
  const parts = columns.map((column) => toDigestPart(row[column] ?? null));
  const folded = content_digest(parts);
  return Number(BigInt.asUintN(53, BigInt.asUintN(64, folded)));
}
