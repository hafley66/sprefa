export interface RustFile {
  path: string;
}

export interface WorktreeEdit {
  path: string;
  digest: string;
  kind: string;
}

export interface WorktreeFile {
  path: string;
  digest: string;
  kind: string;
}
