export interface TreeBranch {
  id: number;
  left: number;
  right: number;
}

export interface TreeKind {
  id: number;
  kind: string;
}

export interface TreeLeaf {
  id: number;
  value: number;
}

export interface TreeTag {
  id: number;
  tag: string;
}
