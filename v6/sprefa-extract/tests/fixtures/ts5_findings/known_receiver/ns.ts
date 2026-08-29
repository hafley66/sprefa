// The module the consumer imports as a namespace. `normalize` is the name the
// receiver check has to keep resolvable.
export function normalize(text: string): string {
    return text.trim();
}
