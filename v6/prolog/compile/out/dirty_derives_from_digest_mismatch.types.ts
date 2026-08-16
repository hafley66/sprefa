export interface Dirty {
  path: string;
}

export interface Head {
  repo_id: number;
  rev_id: number;
}

export interface TreeFile {
  rev_id: number;
  path: string;
  tree_digest: string;
}

export interface WorktreeEdit {
  path: string;
  digest: string;
}

export interface WorktreeFile {
  path: string;
  digest: string;
}
