export interface HostSpan {
  path: string;
  at: Span;
}

export interface HostStart {
  path: string;
  start: number;
}

export interface SourcePath {
  path: string;
}

export interface Span {
  end: number;
  start: number;
}
