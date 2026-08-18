export type GenResultHostErrorBoopResponse5731b3aa340db474 =
  | { tag: 'err'; error: HostError; }
  | { tag: 'ok'; value: BoopResponse; }
;

export type GenResultParseErrorSyntaxTree0284bcd3105168e0 =
  | { tag: 'err'; error: ParseError; }
  | { tag: 'ok'; value: SyntaxTree; }
;

export interface BoopResponse {
  body: string;
}

export interface Compile {
  id: number;
  outcome: GenResultParseErrorSyntaxTree0284bcd3105168e0;
}

export interface Fetch {
  id: number;
  outcome: GenResultHostErrorBoopResponse5731b3aa340db474;
}

export interface HostError {
  code: number;
}

export interface ParseError {
  message: string;
}

export interface SyntaxTree {
  root: string;
}
