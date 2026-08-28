import { Foo } from "./star";

export function viaStar(value: Foo): boolean {
  return value.tag === "foo";
}
