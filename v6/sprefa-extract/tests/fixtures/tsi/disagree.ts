import { render } from "./disagree_callee";

export function drive(): string {
    function render(): string {
        return "local";
    }
    return render();
}
