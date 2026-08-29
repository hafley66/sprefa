import { isIdentifier } from "./shadow_export.js";

export function check(kind: number): boolean {
    return isIdentifier(kind);
}
