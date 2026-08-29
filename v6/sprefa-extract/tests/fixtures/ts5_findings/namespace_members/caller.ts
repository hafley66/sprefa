import { Debug, factory } from "./barrel.js";
import * as barrel from "./barrel.js";

export function build(kind: string): string {
    Debug.assertKind(kind);
    barrel.Debug.assertKind(kind);
    return factory.createLiteral(kind) + barrel.factory.createLiteral(kind);
}
