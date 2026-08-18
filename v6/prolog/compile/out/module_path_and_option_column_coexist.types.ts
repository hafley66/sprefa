export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export interface Labelled {
  tree_id: number;
  state: string;
}

export interface Tree {
  tree_id: number;
  label: number;
}

export interface Orchard {
}
