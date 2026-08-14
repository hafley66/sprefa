export interface CurrentTree {
  path: string;
  digest: string;
}

export interface Head {
  repo_id: number;
  rev_id: number;
}

export interface HeadMove {
  repo_id: number;
  rev_id: number;
}

export interface TreeFile {
  rev_id: number;
  path: string;
  digest: string;
}
