// A second, unimported `normalize`. Its presence is what makes the bare name
// ambiguous to a corpus-wide name match.
export function normalize(count: number): number {
    return count | 0;
}
