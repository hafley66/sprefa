export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export interface Record {
  id: number;
  tag_values: Option<Array<string>>;
  grid_values: Option<Array<Array<string>>>;
  note: Option<string>;
  maybe_tag_values: Array<Option<string>>;
}
