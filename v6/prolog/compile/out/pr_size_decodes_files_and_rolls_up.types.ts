export interface DirFile {
  dir: string;
  path: string;
  adds: number;
  dels: number;
}

export interface DirSize {
  dir: string;
  adds: number;
  dels: number;
  files: number;
}

export interface FilesResp {
  body: unknown;
}

export interface InDir {
  path: string;
  dir: string;
}

export interface PrFile {
  path: string;
  adds: number;
  dels: number;
}
