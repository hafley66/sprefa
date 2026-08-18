/**
 * physicalNames.ts — where a test learns which SQLite object a rel stores in.
 *
 * A stored rel's physical name is `<module>_<rel>_<shape digest>`
 * (docs/storage-name-hash.md), so a check that concatenates the module prefix
 * itself names a table that does not exist. Both readers below take the name
 * from the compiled program, which is the only thing that knows the digest.
 */

import type { IGenProgram } from "../runtime/types.ts";

export function physical_name(program: Pick<IGenProgram, "rel_physical_names">, rel: string): string {
  const name = program.rel_physical_names?.[rel];
  if (name === undefined) throw new Error(`the program carries no physical name for rel ${rel}`);
  return name;
}

/** The same map read out of emitted TEXT, for a check that never imports the
 *  module it is reading. */
export function physical_name_in_source(source: string, rel: string): string {
  const block = source.match(/const rel_physical_names: Record<string, string> = \{([\s\S]*?)\n\};/);
  if (block === null) throw new Error("the emitted module carries no rel_physical_names");
  const row = block[1]!.match(new RegExp(`^\\s*(?:"${rel}"|${rel}): "([^"]+)",$`, "m"));
  if (row === null) throw new Error(`the emitted module carries no physical name for rel ${rel}`);
  return row[1]!;
}
