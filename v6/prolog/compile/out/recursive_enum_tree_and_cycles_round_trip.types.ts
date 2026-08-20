export type Tree =
  | { tag: 'leaf'; value: number; }
  | { tag: 'branch'; left: Tree; right: Tree; }
;

export interface TreeBranch {
  id: number;
  left: Tree;
  right: Tree;
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
