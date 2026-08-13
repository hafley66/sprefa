export interface KnownRepo {
  to_repo_id: number;
}

export interface PinExtracted {
  from_span_id: number;
  to_repo_id: number;
  to_rev_id: number;
  to_path: string;
  kind: string;
}

export interface RepoCandidate {
  to_repo_id: number;
}

export interface Xref {
  from_span_id: number;
  to_repo_id: number;
  to_rev_id: number;
  to_path: string;
  col5: string;
  kind: string;
}
