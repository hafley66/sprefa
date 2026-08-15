export interface Holder {
  id: number;
  nested: OuterPair;
}

export interface InnerPair {
  first: number;
  second: number;
}

export interface OuterPair {
  first: InnerPair;
  second: InnerPair;
}

export interface Touched {
  id: number;
}
