// C5 plane/key inheritance: do the source's MODIFIERS ride the splice, or
// only its members? The dl question is whether key()/keep/log follow columns.

type ReadonlyRow = { readonly id: number; readonly name: string };

declare const source: ReadonlyRow;

// (1) value spread: readonly is DROPPED, the result is mutable
const spread = { ...source, extra: 7 };
spread.id = 99;

// (2) optionality IS carried by the value spread
type OptionalRow = { id?: number };
declare const optSource: OptionalRow;
const optSpread = { ...optSource, extra: 7 };
const optCarried: number = optSpread.id;

// (3) positional splice of a readonly tuple: the result is mutable
type ReadonlyTuple = readonly [id: number, name: string];
type SplicedTuple = [...ReadonlyTuple, extra: number];
const mutableAfterSplice: SplicedTuple = [1, "n", 7];
mutableAfterSplice[0] = 99;

// (4) intersection is the one mechanism that DOES keep readonly
type Intersected = ReadonlyRow & { extra: number };
declare const inter: Intersected;
inter.id = 99;

export { spread, optCarried, mutableAfterSplice };
