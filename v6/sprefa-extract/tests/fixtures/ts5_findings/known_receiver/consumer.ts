// The two receivers the `member-call-unknown-receiver` rule must NOT block: a
// namespace import binding (`helpers`) and `this`. Sits beside
// `receiver_blind_prototype.ts`, which carries the blocked shape.
import * as helpers from "./ns.js";

export class Runner {
    run(text: string): string {
        return helpers.normalize(this.tidy(text));
    }

    tidy(text: string): string {
        return text;
    }
}
