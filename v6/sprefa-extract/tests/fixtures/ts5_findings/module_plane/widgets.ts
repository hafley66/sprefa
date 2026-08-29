// The barrel's second target. Spells a DIFFERENT name, so the barrel is
// unambiguous; `shadowed` is what the star-ambiguity fixture reuses.
export function widen(text: string): string {
    return `${text} `;
}
