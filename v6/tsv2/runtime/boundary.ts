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

const BASE64_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/** RFC 4648 base64, with no platform Buffer dependency. */
export function bytes_to_base64(bytes: Uint8Array): string {
  let out = "";
  for (let index = 0; index < bytes.length; index += 3) {
    const a = bytes[index]!;
    const b = bytes[index + 1];
    const c = bytes[index + 2];
    out += BASE64_ALPHABET[a >> 2];
    out += BASE64_ALPHABET[((a & 3) << 4) | ((b ?? 0) >> 4)];
    out += b === undefined ? "=" : BASE64_ALPHABET[((b & 15) << 2) | ((c ?? 0) >> 6)];
    out += c === undefined ? "=" : BASE64_ALPHABET[c & 63];
  }
  return out;
}

export function base64_to_bytes(text: string): Uint8Array {
  if (text.length % 4 !== 0 || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(text)) {
    throw new Error("invalid_bytes_base64");
  }
  const output = new Uint8Array((text.length / 4) * 3 - (text.endsWith("==") ? 2 : text.endsWith("=") ? 1 : 0));
  let cursor = 0;
  for (let index = 0; index < text.length; index += 4) {
    const a = BASE64_ALPHABET.indexOf(text[index]!);
    const b = BASE64_ALPHABET.indexOf(text[index + 1]!);
    const c = text[index + 2] === "=" ? 0 : BASE64_ALPHABET.indexOf(text[index + 2]!);
    const d = text[index + 3] === "=" ? 0 : BASE64_ALPHABET.indexOf(text[index + 3]!);
    output[cursor++] = (a << 2) | (b >> 4);
    if (cursor < output.length) output[cursor++] = ((b & 15) << 4) | (c >> 2);
    if (cursor < output.length) output[cursor++] = ((c & 3) << 6) | d;
  }
  if (bytes_to_base64(output) !== text) throw new Error("invalid_bytes_base64");
  return output;
}

/** Decode the tagged JSON spelling at a schedule/file boundary. Runtime
 * callers receive Uint8Array values after this function; SQLite and the tick
 * log never see the tagged object. */
export function decode_json_arrivals(
  schedule: unknown,
  rel_column_types: Readonly<Record<string, readonly (string | undefined)[]>>,
): import("./types.ts").IArrivalBatch[] {
  if (!Array.isArray(schedule)) throw new Error("arrival schedule must be an array");
  return schedule.map((batch, batch_index) => {
    if (!Array.isArray(batch)) throw new Error(`arrival batch ${batch_index} must be an array`);
    return batch.map((entry, entry_index) => {
      if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
        throw new Error(`arrival ${batch_index}:${entry_index} must be an object`);
      }
      const arrival = entry as { readonly rel?: unknown; readonly sign?: unknown; readonly row?: unknown };
      if (typeof arrival.rel !== "string" || !Array.isArray(arrival.row)) {
        throw new Error(`arrival ${batch_index}:${entry_index} has invalid shape`);
      }
      const types = rel_column_types[arrival.rel] ?? [];
      const row = arrival.row.map((value, column) => {
        if (types[column] !== "bytes") return value;
        if (typeof value !== "object" || value === null || Array.isArray(value)) {
          throw new Error(`'${arrival.rel}' column ${column} must be a tagged $bytes object`);
        }
        const encoded = value as { readonly $bytes?: unknown };
        if (typeof encoded.$bytes !== "string" || Object.keys(value).length !== 1) {
          throw new Error(`'${arrival.rel}' column ${column} must be a tagged $bytes object`);
        }
        return base64_to_bytes(encoded.$bytes);
      });
      return { rel: arrival.rel, sign: arrival.sign, row } as import("./types.ts").IArrivalBatch[number];
    });
  });
}
