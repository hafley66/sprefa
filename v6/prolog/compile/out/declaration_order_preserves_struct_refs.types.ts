export interface BoxList {
  tree_id: number;
  items: Array<string>;
}

export interface Patch {
  label: string;
  at: Plot;
}

export interface Plot {
  row: number;
  col: number;
}

export interface Tree {
  tree_id: number;
  species: string;
  site: Patch;
}

export interface TreeLabel {
  tree_id: number;
  label: string;
}
