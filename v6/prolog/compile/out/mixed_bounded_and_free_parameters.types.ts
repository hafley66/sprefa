export interface JsonEncodable {}

export interface Entry<Key extends JsonEncodable, Value> {
  key: Key;
  value: Value;
}

export interface GenEntryTextIntA6c3f6c7e60e6b95 {
  key: string;
  value: number;
}

export interface Carry {
  id: number;
  slot: GenEntryTextIntA6c3f6c7e60e6b95;
}

export interface Cell {
  id: number;
  slot: GenEntryTextIntA6c3f6c7e60e6b95;
}
