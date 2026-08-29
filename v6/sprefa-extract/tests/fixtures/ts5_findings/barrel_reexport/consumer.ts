// Imports `normalize` through the barrel, so the import names exactly one file.

import { normalize } from "./barrel.js";

export function run(text: string): string {
    return normalize(text);
}
