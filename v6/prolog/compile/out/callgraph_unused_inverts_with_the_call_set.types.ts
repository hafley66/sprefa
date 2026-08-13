export interface Call {
  path: string;
  callee: string;
}

export interface Def {
  path: string;
  name: string;
  kind: string;
}

export interface NodeFact {
  path: string;
  record: string;
  kind: string;
  name: string;
}

export interface Unused {
  name: string;
}
