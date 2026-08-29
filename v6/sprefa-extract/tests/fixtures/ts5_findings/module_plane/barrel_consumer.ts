// Two names through one barrel, each reaching exactly one file.
import { normalize, widen } from "./index.js";

export function run(text: string): string {
    return widen(normalize(text));
}
