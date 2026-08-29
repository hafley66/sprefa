import { Context } from "./ctx.js";

export function transform(context: Context): void {
    const { emitter } = context;
    emitter.writeLine("via destructuring");
    context.emitter.writeLine("via member read");
}

export function transformRenamed(context: Context): void {
    const { emitter: sink } = context;
    sink.writeLine("via renamed destructuring");
}
