import { Holder, Session } from "./api.js";

export function runHolder(holder: Holder): void {
    holder.session.start();
    holder.session.stop();
}

export function runSession(session: Session): void {
    session.start();
    session.stop();
}
