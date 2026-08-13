export interface Grew {
  orchard_id: number;
  tree_id: number;
  branch_id: number;
}

export interface Orchard {
  orchard_id: number;
}

export interface Tree {
  parent: Orchard;
  tree_id: number;
}

export interface Branch {
  parent: Tree;
  branch_id: number;
}
