// Const-facet parity fixture. Exercises string/template/object-dotted-path/
// string-enum/as-const/mutable-skip/numeric-skip/nested-scope, mirroring v5's
// own const tests so the v5 oracle is the trusted baseline.

// bare string const -> Const entity + const_value(home, "", "/home", lit)
export const home = "/home";

// object literal -> Const entity + dotted-path values
export const routes = { home: "/home", api: "/api", nested: { a: "/a", b: "/b" } };

// template const -> Const entity + const_value(greeting, "", raw slice, template)
export const greeting = `hi ${name}`;

// numeric const -> NO entity, NO const_value (no string anywhere)
export const count = 3;

// string enum -> enum is an Enum entity; const_value rows keyed by the enum,
// field = member name
export enum Routes {
  Home = "/home",
  About = "/about",
  Numeric = 7,
}

// `as const` on a let is honest to fold; plain let is a mutable-skip.
export let mutablePath = "/mut";
export const pinned = "/pin" as const;

// arrow const -> Function entity (TypeProjector), NOT a const entity
export const handler = (x: number) => x + 1;

// nested const inside a function body -> a Const entity scoped by span
export function makeTable() {
  const INNER_TABLE = { x: "/inner/x", y: "/inner/y" };
  return INNER_TABLE;
}
