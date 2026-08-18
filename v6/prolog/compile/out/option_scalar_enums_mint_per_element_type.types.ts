export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export interface Measurement {
  sensor_id: number;
  label: Option<string>;
  reading: Option<number>;
}
