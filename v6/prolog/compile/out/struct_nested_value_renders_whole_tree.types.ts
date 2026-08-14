export interface Diag {
  where: Place;
  message: string;
}

export interface DiagFile {
  file: string;
}

export interface Place {
  file: string;
  at: Span;
}

export interface Span {
  start: number;
  end: number;
}
