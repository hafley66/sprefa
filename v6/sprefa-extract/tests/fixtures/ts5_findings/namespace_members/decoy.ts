export function assertKind(kind: string): void {
    void kind;
}

export class DecoyFactory {
    createLiteral(text: string): string {
        return text;
    }
}
