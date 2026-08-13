export interface Finding {
  at: Span;
}

export interface Span {
  start: number;
  end: number;
}

export interface SpanSeen {
  start: number;
  end: number;
}
