export interface Coord {
  path_name: string;
  start: number;
  end: number;
}

export interface File {
  repo: Repo;
  at: Fpath;
}

export interface Fpath {
  name: string;
}

export interface Raw {
  repo_name: string;
  path_name: string;
  start: number;
  end: number;
}

export interface Repo {
  name: string;
}

export interface Span {
  file: File;
  start: number;
  end: number;
}
