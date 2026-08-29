// Module-private `isIdentifier`, never exported, never importable.

function isIdentifier(): boolean {
    return false;
}

export function parse(): boolean {
    return isIdentifier();
}
