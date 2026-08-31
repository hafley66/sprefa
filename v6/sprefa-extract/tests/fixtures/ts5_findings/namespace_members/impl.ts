import { Factory } from "./api.js";

export namespace Debug {
    export function assertKind(kind: string): void {
        if (kind === "") {
            throw new Error("empty");
        }
    }
}

export const factory: Factory = makeFactory();

function makeFactory(): Factory {
    return { createLiteral: (text: string) => text };
}
