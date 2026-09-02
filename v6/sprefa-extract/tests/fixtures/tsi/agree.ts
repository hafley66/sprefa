import { Base, helper } from "./agree_callee";

export class Child extends Base {}

export function run(): number {
    return helper();
}

export function seat(base: Base): string {
    return base.greet();
}
