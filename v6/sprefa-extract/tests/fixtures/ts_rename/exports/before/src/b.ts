import { Foo as Bar } from "./lib";

export function secondUse(left: Bar, right: Bar): Bar {
  return left.tag === right.tag ? left : right;
}
