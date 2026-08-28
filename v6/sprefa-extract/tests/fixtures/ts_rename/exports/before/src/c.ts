import { Foo } from "./barrel";

export function fromBarrel(value: Foo): string {
  return value.tag;
}
