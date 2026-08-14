export interface EprintlnCount {
  path: string;
  col2: number;
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

export interface WaiverBlockComment {
  path: string;
  waiver_line: string;
}

export interface WaiverTrailingComment {
  path: string;
  waiver_line: number;
}
