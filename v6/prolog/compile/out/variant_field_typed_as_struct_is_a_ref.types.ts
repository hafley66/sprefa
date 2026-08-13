export interface Holder {
  item: Span;
}

export interface LocElsewhere {
  id: number;
  note: string;
}

export interface LocHere {
  id: number;
  at: Span;
}

export interface LocTag {
  id: number;
  tag: string;
}

export interface Span {
  lo: number;
  hi: number;
}
