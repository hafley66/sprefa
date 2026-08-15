export interface Kept {
  id: number;
}

export interface MixedPair {
  first: PointPair;
  label: string;
}

export interface PointPair {
  first: number;
  depth: number;
}

export interface Record {
  id: number;
  nested: MixedPair;
}
