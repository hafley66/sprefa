/**
 * Read the boot DDL out of an emitted tsv2 module.
 *
 * The module holds it as one array literal `const ddl: readonly string[] = [ ... ];`
 * whose elements are backtick template literals. Collect every literal between
 * the opening bracket and its match; an interpolation would make the text
 * non-static and throws.
 */

export function extractDdl(source: string): readonly string[] {
  const marker = "const ddl: readonly string[] = [";
  const start = source.indexOf(marker);
  if (start < 0) throw new Error("no ddl array in module");
  const statements: string[] = [];
  let depth = 0;
  for (let cursor = start + marker.length - 1; cursor < source.length; cursor += 1) {
    const character = source[cursor];
    if (character === "`") {
      const close = source.indexOf("`", cursor + 1);
      if (close < 0) throw new Error("unterminated template literal");
      const body = source.slice(cursor + 1, close);
      if (body.includes("${")) throw new Error("interpolated ddl statement");
      statements.push(body);
      cursor = close;
      continue;
    }
    if (character === "[") depth += 1;
    if (character === "]") {
      depth -= 1;
      if (depth === 0) return statements;
    }
  }
  throw new Error("unterminated ddl array");
}

/** The object a CREATE/INSERT names, taken from its first quoted identifier. */
export function ddlTarget(statement: string): string {
  return /"([^"]+)"/.exec(statement)?.[1] ?? "";
}
