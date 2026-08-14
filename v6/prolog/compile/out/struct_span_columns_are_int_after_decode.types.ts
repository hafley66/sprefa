export interface DefStart {
  path: string;
  offset: number;
}

export interface NodeFact {
  path: string;
  name: string;
  at: Span;
}

export interface Span {
  end: number;
  start: number;
}
