export interface Labelled {
  tree_id: number;
  state: string;
}

export interface Orchard {
  orchard_id: number;
}

export interface Tree {
  parent: Orchard;
  tree_id: number;
  label: number;
}

export interface Planted {
  orchard_id: number;
  tree_id: number;
}
