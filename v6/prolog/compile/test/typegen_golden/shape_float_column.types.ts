export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export interface Measurement {
  id: number;
  ratio: number;
  label: string;
  samples: Array<number>;
  margin: Option<number>;
}
