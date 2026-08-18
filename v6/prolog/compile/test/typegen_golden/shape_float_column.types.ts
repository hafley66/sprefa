export interface Measurement {
  id: number;
  ratio: number;
  label: string;
  samples: Array<number>;
  margin: number | null;
}
