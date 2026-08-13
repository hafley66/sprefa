export interface ChangedFile {
  path: string;
}

export interface Diag {
  path: string;
  line_no: number;
  col3: string;
  col4: string;
  col5: string;
  col: string;
  end_col: string;
}

export interface EprintlnBaseline {
  path: string;
  allowed: number;
}

export interface EprintlnCount {
  path: string;
  hits: number;
}

export interface EprintlnCounted {
  path: string;
  line_no: number;
}

export interface EprintlnHit {
  path: string;
  line_no: number;
}

export interface EprintlnWaived {
  path: string;
  line_no: number;
}

export interface EprintlnWaiverLine {
  path: string;
  waiver_line: number;
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

export interface WaiverBlockComment {
  path: string;
  waiver_line: string;
}

export interface WaiverTrailingComment {
  path: string;
  waiver_line: number;
}
