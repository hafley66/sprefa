export interface AnyDiag {
  name: string;
}

export interface CheckExit {
  name: string;
  col2: number;
}

export interface Diag {
  path: string;
  line_no: number;
  col3: string;
  col4: string;
  col5: string;
  col6: string;
  col7: string;
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

export interface Program {
  name: string;
}

export interface WaiverBlockComment {
  path: string;
  waiver_line: string;
}

export interface WaiverTrailingComment {
  path: string;
  waiver_line: number;
}
