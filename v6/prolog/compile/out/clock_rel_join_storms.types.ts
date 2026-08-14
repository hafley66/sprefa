export interface DiagHistory {
  path: string;
  line: number;
  code: string;
  opened_at: number;
}

export interface DiagSeen {
  path: string;
  line: number;
  code: string;
  at: number;
}

export interface Diagnostic {
  path: string;
  line: number;
  code: string;
  col4: string;
}

export interface FileLine {
  path: string;
  line: number;
  code: string;
}

export interface Ratchet {
  col1: string;
  col2: number;
}

export interface TickRel {
  at: number;
}
