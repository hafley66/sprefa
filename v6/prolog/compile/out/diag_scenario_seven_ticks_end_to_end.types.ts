export interface DiagHistory {
  path: string;
  line: number;
  code: string;
  opened_at: number;
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

export interface HookWindow {
  turn: string;
  since: number;
}

export interface LintCount {
  code: string;
  count: number;
}

export interface Ratchet {
  code: string;
  allowed: number;
}

export interface TurnDiag {
  turn: string;
  path: string;
  line: number;
  code: string;
  opened_at: number;
}

export interface UnratchetedLint {
  code: string;
  count: number;
}

export interface Violation {
  code: string;
  count: number;
  allowed: number;
}
