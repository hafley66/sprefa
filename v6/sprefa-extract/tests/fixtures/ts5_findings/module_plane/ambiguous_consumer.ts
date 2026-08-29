import { collide } from "./ambiguous_barrel.js";

export function pick(text: string): string {
    return collide(text);
}
