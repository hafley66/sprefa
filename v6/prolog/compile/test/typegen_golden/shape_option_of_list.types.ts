export interface Record {
  id: number;
  tag_values: Array<string> | null;
  grid_values: Array<Array<string>> | null;
  note: string | null;
  maybe_tag_values: Array<string | null>;
}
