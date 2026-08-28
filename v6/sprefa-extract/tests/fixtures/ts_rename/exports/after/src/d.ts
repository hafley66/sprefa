import { Baz } from "./star";

export function viaStar(value: Baz): boolean {
  return value.tag === "foo";
}
