import "./0_spans.tsp";

namespace Trial;

// Storage fixtures only. The spread reuses the authored span fields.
// These rows have no source identity and are not the extraction graph schema.
model SpanRow {
  @Entity.pk id: int64;
  ...Extract.Span;
}

model SpanOutRow {
  @Entity.pk id: int64;
  ...Extract.SpanOut;
}
