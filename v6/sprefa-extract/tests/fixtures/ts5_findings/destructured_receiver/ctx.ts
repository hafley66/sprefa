export interface Emitter {
    writeLine(text: string): void;
}

export interface Context {
    emitter: Emitter;
    depth: number;
}
