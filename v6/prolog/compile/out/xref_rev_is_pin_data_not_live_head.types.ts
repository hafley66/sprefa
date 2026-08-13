export interface Head {
  repo_id: number;
  rev_id: number;
}

export interface HeadMove {
  repo_id: number;
  rev_id: number;
}

export interface KnownRepo {
  col1: number;
}

export interface PinExtracted {
  from_span_id: number;
  to_repo_id: number;
  to_rev_id: number;
  to_path: string;
  kind: string;
}

export interface Xref {
  from_span_id: number;
  to_repo_id: number;
  to_rev_id: number;
  to_path: string;
  col5: string;
  kind: string;
}
