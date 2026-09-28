// Mirror of 2026-09-28-comptime-emit-target.dl7 in TypeSpec terms.

// --- schema (dl7: the `:` declarations) ---
model User {
  id: string;
  name: string;
  age: int32;
}

model Post {
  id: string;
  author: User;
  body: string;
}

// --- gameplan as data (dl7: Step / Needs / Done facts, Ready / Blocked rules) ---
enum StepName {
  sumMatch: "sum-match",
  effectsSum: "effects-sum",
  emitSlice: "emit-slice",
  fsFile: "fs-file",
  dl8Build: "dl8-build",
  strJoin: "str-join",
  docSum: "doc-sum",
  docLayout: "doc-layout",
  refkey: "refkey",
  imports: "imports",
  demoDts: "demo-dts",
  watch: "watch",
}

model Step {
  name: StepName;
  what: string;
  needs: StepName[];
  done: boolean;
}

// --- where each dl7 piece lives in a TypeSpec project ---
model Mapping {
  dl7: string;
  typespec: string;
}

const mappings: Mapping[] = #[
  #{ dl7: "(: Doc (+ text concat nest group line))", typespec: "Alloy component tree (JSX), in JS" },
  #{ dl7: "(<- (iface_doc ?T ?Doc) ...)", typespec: "function Interface({ type }) returning <ts.InterfaceDeclaration/>" },
  #{ dl7: "(name_of ?T ?Name)", typespec: "refkey(type) + the binder" },
  #{ dl7: "needs_import rule", typespec: "Alloy auto-imports from refkeys across SourceFiles" },
  #{ dl7: "(doc.layout ?Doc 80 ?Text)", typespec: "Alloy printer (prettier-style layout)" },
  #{ dl7: "(fs.file ?Path ?Text)", typespec: "<SourceFile path=...> + emitFile" },
  #{ dl7: "dl8 build: settle, write, exit", typespec: "tsp compile: check, $onEmit, render once" },
  #{ dl7: "dl8 run: same rules stay live", typespec: "tsp compile --watch reruns the whole emit" },
];
