export interface Hit {
  owner: string;
  at: Span;
}

export interface Span {
  start: number;
  end: number;
}
