export interface Orchard {
  orchard_id: number;
}

export interface Tree {
  tree_id: number;
}

export interface PerOrchard {
  orchard_id: number;
  trees: number;
}

export interface Planted {
  orchard_id: number;
  tree_id: number;
}
