// A second file spelling `normalize`, exported and never imported by the
// consumer. Its presence is what makes the bare name ambiguous.

export function normalize(n: number): number {
    return n | 0;
}
