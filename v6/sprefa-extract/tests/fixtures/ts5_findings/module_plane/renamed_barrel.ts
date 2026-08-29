// A renaming re-export: the consumer asks for `outer`, the file declares `inner`.
export { inner as outer } from "./renamed_source.js";
