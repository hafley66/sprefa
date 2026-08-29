import { Session } from "./api.js";

export class ClassSession implements Session {
    start(): void {}
    stop(): void {}
}

export function createLiteralSession(): Session {
    return {
        start(): void {},
        stop(): void {},
    };
}
