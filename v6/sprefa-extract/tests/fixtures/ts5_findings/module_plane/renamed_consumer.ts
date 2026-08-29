import { outer } from "./renamed_barrel.js";

export function callIt(text: string): string {
    return outer(text);
}
