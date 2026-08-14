export interface AnyDiag {
  name: string;
}

export interface ChangedFile {
  path: string;
}

export interface CheckExit {
  name: string;
  col2: number;
}

export interface Diag {
  path: string;
  line_no: number;
  severity: string;
  code: string;
  col5: string;
  col: string;
  end_col: string;
}

export interface DiagStage {
  code: string;
  stage: string;
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

export interface GateBlocked {
  stage: string;
}

export interface GateExit {
  stage: string;
  col2: number;
}

export interface GateThreshold {
  stage: string;
  min_rank: number;
}

export interface Program {
  name: string;
}

export interface SeverityRank {
  severity: string;
  rank: number;
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
