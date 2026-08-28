import { Baz } from "./barrel";

export function fromBarrel(value: Baz): string {
  return value.tag;
}
