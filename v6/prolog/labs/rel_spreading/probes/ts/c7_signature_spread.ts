// C7 spread inside a FUNCTION SIGNATURE (the host-decl analog): does a
// signature keep an input/output split when its inputs are spliced?

type CommonInputs = [repo: string, rev: string];

// (1) inputs spliced into the parameter list, one extra input appended,
// outputs untouched. Accepted.
declare function fetchRow(
  ...args: [...CommonInputs, endpoint: string]
): { status: number; body: string };

const out = fetchRow("r", "abc", "/stars");
const statusIsNumber: number = out.status;

// (2) width is enforced at the call: one input short
const short = fetchRow("r", "abc");

// (3) the OUTPUT side has no positional spread mechanism at all; splicing a
// return shape uses intersection, and the two sides therefore use different
// spellings
type CommonOutputs = { status: number };
declare function fetchRow2(
  ...args: [...CommonInputs, endpoint: string]
): CommonOutputs & { body: string };
const out2 = fetchRow2("r", "abc", "/stars");
const bodyIsString: string = out2.body;

export { out, statusIsNumber, short, out2, bodyIsString };
