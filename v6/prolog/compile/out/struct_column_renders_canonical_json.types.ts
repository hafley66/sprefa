export interface Finding {
  path: string;
  at: Span;
}

export interface Span {
  start: number;
  end: number;
}

export interface Touched {
  path: string;
}
