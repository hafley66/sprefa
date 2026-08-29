// Corpus finding, NOT FIXED: `call_name_match` matches the callee SPELLING
// against the corpus def index and never looks at the receiver, so a builtin
// prototype call binds to any class method that happens to share the name. One
// class method named `push` anywhere in the resolution universe captures every
// `Array.prototype.push` call in it.
//
// This is the WRONG-FACT face of the documented diet ambiguity: the doc says an
// ambiguous name yields no edge, and a name unique to one blob yields an edge.
// A unique-but-unrelated method makes that edge point at the wrong definition
// rather than at nothing.
//
// Measured on microsoft/TypeScript @9a8581c3: 642 of 8025 resolved_edge rows
// (8.0%) name a JS builtin prototype method or global. The leaders are
// `push` -> tools/scripts/tsc/generate-encoder.ts (87 edges),
// `add` -> src/api/node/encoder.ts (114) and `at` (35).
//
// Repro:
//   extract --resolve --family call consumer.ts writer.ts
// Expected: zero resolved_edge rows out of collect (Array.prototype.push is
// outside the universe, so the documented answer is no edge).
// Observed: two resolved_edge rows, consumer.ts:collect -> writer.ts:push.
export function collect(items: string[]): string[] {
    const out: string[] = [];
    out.push(items[0]);
    out.push(items[1]);
    return out;
}
