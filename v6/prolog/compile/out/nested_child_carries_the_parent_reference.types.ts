export interface Orchard {
  orchard_id: number;
}

export interface Tree {
  parent: Orchard;
  tree_id: number;
}

export interface Planted {
  orchard_id: number;
  tree_id: number;
}
