// Imports the exported spelling by name.

import { isIdentifier } from "./nodeTests.js";

export function check(kind: number): boolean {
    return isIdentifier(kind);
}
