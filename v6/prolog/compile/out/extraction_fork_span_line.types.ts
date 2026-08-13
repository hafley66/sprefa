export interface File {
  file: string;
  file_digest: string;
}

export interface QueryValue {
  query_digest: string;
}

export interface SpanLine {
  file: string;
  line: number;
  text: string;
}
