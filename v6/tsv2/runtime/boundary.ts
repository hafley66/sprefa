/** Mirrors engine-rs `BoundaryError::ListAtScalarSeam` + `ScalarSeam`:
 *  both doors must answer a program with the same Display bytes. */
export type ScalarSeam =
  | "sql_parameter"
  | "host_template_argument"
  | "arrival_payload"
  | "text_intern";

const SCALAR_SEAM_NAMES: Readonly<Record<ScalarSeam, string>> = {
  sql_parameter: "a SQL parameter",
  host_template_argument: "a host template argument",
  arrival_payload: "an arrival payload",
  text_intern: "the text intern plane",
};

/** A list value that crossed a runtime boundary; catch by `instanceof`. */
export class BoundaryError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "BoundaryError";
  }
}

/** `BoundaryError::ListAtScalarSeam`: "a list value reached <seam>". */
export function list_at_scalar_seam(seam: ScalarSeam): BoundaryError {
  return new BoundaryError(`a list value reached ${SCALAR_SEAM_NAMES[seam]}`);
}
