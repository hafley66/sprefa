import { deep } from "./two_hop_outer.js";

export function reach(text: string): string {
    return deep(text);
}
