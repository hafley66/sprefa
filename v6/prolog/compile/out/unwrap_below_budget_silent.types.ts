export interface ChangedFile {
  path: string;
}

export interface Diag {
  path: string;
  line_no: number;
  col3: string;
  col4: string;
  col5: string;
  col: number;
  end_col: number;
}

export interface UnwrapCount {
  path: string;
  total: number;
}

export interface UnwrapHit {
  path: string;
  line_no: number;
  col: number;
  end_col: number;
}
