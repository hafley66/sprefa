export interface Session {
    start(): void;
}

export interface Session {
    stop(): void;
}

export interface Holder {
    session: Session;
}
