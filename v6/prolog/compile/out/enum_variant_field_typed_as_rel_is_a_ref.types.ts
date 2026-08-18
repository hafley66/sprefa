export type Grade =
  | { tag: 'ripe'; subject: Tree; }
  | { tag: 'bruised'; reason: string; }
;

export interface GradeBruised {
  id: number;
  reason: string;
}

export interface GradeRipe {
  id: number;
  subject: Tree;
}

export interface GradeTag {
  id: number;
  tag: string;
}

export interface Graded {
  id: number;
  g: Grade;
}

export interface GradedTag {
  id: number;
  tag: string;
}

export interface Tree {
  tree_id: number;
  name: string;
}
