// Corpus finding, NOT FIXED: a class FIELD initialized with an arrow function
// is not minted as a call def, so its body's calls have no covering def and
// `--resolve` emits nothing for them. `lambda_entry_class`
// (src/lang/ts.rs:1610) walks ClassElement::MethodDefinition only; a
// PropertyDefinition holding an arrow is skipped, and the header at
// src/lang/ts.rs:1570-1574 names field initializers as an intentional v5
// parity exclusion.
//
// The bound-handler field is the standard shape for a callback that must keep
// `this`. microsoft/TypeScript @9a8581c3 happens to use none, so this fixture
// carries a corpus count of ZERO and stands on the repro below alone.
//
// Repro:
//   extract --family call class_field_initializer.ts
// Expected call defs: target, method, and the `handler` arrow.
// Observed: target and method only.
// Expected resolved_edge rows under `--resolve`: two.
// Observed: one, Runner:method -> target.
export function target(n: string): string {
    return n;
}

export class Runner {
    handler = (n: string): string => {
        return target(n);
    };

    method(n: string): string {
        return target(n);
    }
}
