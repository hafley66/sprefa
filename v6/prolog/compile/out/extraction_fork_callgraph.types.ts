export interface CallEdge {
  file: string;
  caller: string;
  callee: string;
  start: number;
  end: number;
}

export interface CallSite {
  file: string;
  caller: string;
  callee: string;
}

export interface File {
  file: string;
  file_digest: string;
}

export interface QueryValue {
  query_digest: string;
}
