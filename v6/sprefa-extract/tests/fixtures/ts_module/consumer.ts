import { alpha } from "./barrel.js";
import "./side_effect.js";
import helper = require("./cjs_helper");
export * from "./star_target.js";
export { beta } from "./beta.js";

export function consume(): void {
    alpha();
    helper.helper();
    void import("./lazy.js");
}
