export interface Audit {
  audit_id: number;
  at_commit: Commit;
}

export interface Commit {
  id: number;
}

export interface Person {
  id: number;
  name: string;
}

export interface Reviewed {
  commit_id: number;
  reviewer_name: string;
}
