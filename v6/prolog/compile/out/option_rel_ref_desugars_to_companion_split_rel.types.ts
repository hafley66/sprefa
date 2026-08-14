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
