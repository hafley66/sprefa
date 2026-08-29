// One of two files exporting `collide`. Star-exported beside the other, the
// name has two different bindings and ResolveExport is AMBIGUOUS.
export function collide(text: string): string {
    return text;
}
