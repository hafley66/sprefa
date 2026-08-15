export interface Tensor {
  id: number;
  row_values: Array<string>;
  grid_values: Array<Array<string>>;
  cube_values: Array<Array<Array<string>>>;
  hypercube_values: Array<Array<Array<Array<string>>>>;
}
