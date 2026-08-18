export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export interface Squad {
  id: number;
  rank: Option<Option<number>>;
}
