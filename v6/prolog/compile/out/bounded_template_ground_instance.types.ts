export interface JsonEncodable {}

export interface Pair<T extends JsonEncodable> {
  first: T;
  second: T;
}

export interface GenPairInt8b7ec0fa0e1f9d69 {
  first: number;
  second: number;
}

export interface Carry {
  id: number;
  endpoints: GenPairInt8b7ec0fa0e1f9d69;
}

export interface Edge {
  id: number;
  endpoints: GenPairInt8b7ec0fa0e1f9d69;
}
