export interface JsonEncodable {}

export interface Couple<Left extends JsonEncodable, Right extends JsonEncodable> {
}

export interface Wrap<T extends JsonEncodable> {
  value: T;
}

export interface GenCoupleWrapIntWrapTextFea7bde20e4f244e {
  first: GenWrapInt74568235536ee9d4;
  second: GenWrapText2bd6acc46ade78fd;
}

export interface GenWrapInt74568235536ee9d4 {
  value: number;
}

export interface GenWrapText2bd6acc46ade78fd {
  value: string;
}

export interface Carry {
  id: number;
  nested: GenCoupleWrapIntWrapTextFea7bde20e4f244e;
}

export interface Index {
  id: number;
  nested: GenCoupleWrapIntWrapTextFea7bde20e4f244e;
}
