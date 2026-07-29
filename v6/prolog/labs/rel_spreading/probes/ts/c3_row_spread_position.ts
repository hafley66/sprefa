// C3 row spread in a head position: can a spread be followed by explicit
// positional arguments, and where does the width come from?

type ARow = [id: number, name: string];

// (1) a spread with trailing explicit elements in a TYPE position: accepted
type CRow = [...ARow, extra: number];
declare const aRow: ARow;
const cFromSpread: CRow = [...aRow, 5];

// (2) the same shape in a CALL: accepted only because the spread argument has
// a tuple type. The width is read from the tuple type, not from the value.
declare function takesThree(id: number, name: string, extra: number): void;
takesThree(...aRow, 5);

// (3) an ARRAY (unknown width) in the same position: refused
declare const aArray: number[];
declare function takesTwoNumbers(x: number, y: number): void;
takesTwoNumbers(...aArray);

// (4) each spliced position is an independent slot, not one aggregate value
const idSlot: number = cFromSpread[0];
const nameSlot: string = cFromSpread[1];

export { cFromSpread, idSlot, nameSlot };
