export interface JsonEncodable {}

export interface Mapper<In extends JsonEncodable, Out extends JsonEncodable> {
  input: In;
  return: Out;
}

export interface GenMapperIntText27b8a56119fbf234 {
  input: number;
  return: string;
}

export interface Carry {
  id: number;
  applied: GenMapperIntText27b8a56119fbf234;
}

export interface Conversion {
  id: number;
  applied: GenMapperIntText27b8a56119fbf234;
}
