export interface CalleeSetSize {
  left: string;
  left_size: number;
}

export interface Jaccard {
  left: string;
  right: string;
  col3: number;
}

export interface SharedCount {
  left: string;
  right: string;
  shared: number;
}

export interface UnionSize {
  left: string;
  right: string;
  union: number;
}
