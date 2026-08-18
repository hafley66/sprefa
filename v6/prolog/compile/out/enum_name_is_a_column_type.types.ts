export type Grade =
  | { tag: 'ripe'; sugar: number; }
  | { tag: 'green'; days: number; }
;

export interface GradeGreen {
  id: number;
  days: number;
}

export interface GradeRipe {
  id: number;
  sugar: number;
}

export interface GradeTag {
  id: number;
  tag: string;
}

export interface Picked {
  id: number;
  g: Grade;
}

export interface PickedTag {
  id: number;
  tag: string;
}
