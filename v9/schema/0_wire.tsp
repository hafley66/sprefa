// v9 HTTP boundary. Rust (src/_9_wire/) and OpenAPI (openapi.yaml) are generated from this file
// by 1_generate.mjs.
import "@typespec/http";

using Http;

@service(#{ title: "v9" })
namespace V9;

/** One column value. Exactly one of `text` or `int` is set. */
model Cell {
  text?: string;
  int?: int64;
}

/** One row change of one relation, by relation name. */
model Change {
  rel: string;
  cells: Cell[];
  diff: int64;
}

/** Every relation's row changes at one event. */
model Delta {
  event: uint64;
  changes: Change[];
}

model Table {
  columns: string[];
  rows: Cell[][];
}

enum ColType {
  Text,
  Int,
}

model Column {
  name: string;
  col_type: ColType;
}

enum RelKind {
  Base,
  Derived,
  Request,
  Answer,
}

model Relation {
  name: string;
  cols: Column[];
  kind: RelKind;

  /** Service name, set for Request and Answer relations. */
  service?: string;
}

model Plan {
  relations: Relation[];
}

model QueryRequest {
  sql: string;
}

model Recipe {
  /** Canonical recipe text, e.g. `engine=dd,store=sqlite,wire=http`. */
  canonical: string;
}

@route("/commit") @post op commit(@body changes: Change[]): Delta;
@route("/query") @post op query(@body request: QueryRequest): Table;
@route("/plan") @get op plan(): Plan;
@route("/recipe") @get op recipe(): Recipe;
