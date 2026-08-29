// The user API the receiver rule must NOT block. `getTypeChecker` is a free
// function reached as a property of a factory object, the shape TypeScript's
// whole compiler API has; 173 such edges over `src/**`.
export function getTypeChecker(): number {
    return 1;
}
