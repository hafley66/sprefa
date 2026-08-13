export interface Fpath {
  name: string;
}

export interface Loc {
  at: Fpath;
  line: number;
}

export interface Raw {
  path: string;
  line: number;
}

export interface Seen {
  at: Fpath;
}
