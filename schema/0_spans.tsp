namespace Extract;

/** Local byte range, matching extract::types::Span. End is start + len. */
model Span {
  start: uint32;
  len: uint32;
}

/** Wire byte range, matching extract::types::SpanOut. End is exclusive. */
model SpanOut {
  start: uint32;
  end: uint32;
}
