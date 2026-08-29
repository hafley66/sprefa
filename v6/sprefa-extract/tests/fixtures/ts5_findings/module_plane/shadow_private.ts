// A module-private `isIdentifier`, never exported, never importable. A
// corpus-wide name match cannot tell it from the exported one.
function isIdentifier(): boolean {
    return false;
}

export function parse(): boolean {
    return isIdentifier();
}
