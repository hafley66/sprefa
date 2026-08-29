// Corpus finding: a `namespace` / `declare module` / `declare global` body is a
// TSModuleDeclaration, and every declaration nested inside it must reach the
// type and call families exactly as a top-level one does.
//
// Expected type entities, in source order:
//   function nsFunc, interface NsIface, class NsClass, method run,
//   alias NsAlias, enum NsEnum, function deepFn, function ambientFn,
//   interface Window
// Expected call defs: nsFunc, run, deepFn
// (`ambientFn` is bodiless, so it carries no call def, same as a top-level
// `declare function`.)
export namespace Outer {
  export function nsFunc(v: number): number {
    return v;
  }
  export interface NsIface {
    q: string;
  }
  export class NsClass {
    run(): void {}
  }
  export type NsAlias = string;
  export enum NsEnum {
    Red = "red",
  }
  export namespace Inner {
    export function deepFn(): void {}
  }
}

declare module "ambient" {
  export function ambientFn(): void;
}

declare global {
  interface Window {
    custom: number;
  }
}
