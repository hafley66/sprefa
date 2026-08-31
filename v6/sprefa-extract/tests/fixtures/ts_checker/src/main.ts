import { Panel } from "./panel";
import { pick } from "./pick";
import { Widget } from "./widget";

export function drive(widgets: Widget[]): string {
    const chosen = pick(widgets);
    return chosen.render();
}

export function seat(panels: Panel[]): string {
    return panels[0].render();
}

export function check(text: string): boolean {
    return isNaN(Number(text));
}
