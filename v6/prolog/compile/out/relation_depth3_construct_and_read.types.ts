export interface File {
  repo: Repo;
  at: Fpath;
}

export interface Found {
  path_name: string;
  kind: string;
}

export interface Fpath {
  name: string;
}

export interface Located {
  span: Span;
  kind: string;
}

export interface Rawk {
  repo_name: string;
  path_name: string;
  start: number;
  end: number;
  kind: string;
}

export interface Repo {
  name: string;
}

export interface Span {
  file: File;
  start: number;
  end: number;
}
