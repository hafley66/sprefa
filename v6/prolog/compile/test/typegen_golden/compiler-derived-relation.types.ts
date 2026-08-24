export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export interface Holder {
  value: GenPartialUser9d7a703929b72789;
}

export interface User {
  id: number;
  name: string;
}

export interface GenPartialUser9d7a703929b72789 {
  id: Option<number>;
  name: Option<string>;
}
