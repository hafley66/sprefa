export interface JsonEncodable {}

export interface Pair<T extends JsonEncodable> {
  first: T;
  second: T;
}

export interface Box<V> {
  value: V;
  label: string;
}
