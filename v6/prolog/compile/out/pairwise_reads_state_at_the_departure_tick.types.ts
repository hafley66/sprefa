export interface Reading {
  sensor: string;
  previous: number;
}

export interface Step {
  sensor: string;
  previous: number;
  current: number;
}
