export interface JsonEncodable {}

export interface Span<Start extends JsonEncodable, Label extends JsonEncodable> {
  start: Start;
  label: Label;
}

export interface GenSpanIntTextE5126de851365aff {
  start: number;
  label: string;
}

export interface Carry {
  id: number;
  extent: GenSpanIntTextE5126de851365aff;
}

export interface Marker {
  id: number;
  extent: GenSpanIntTextE5126de851365aff;
}
