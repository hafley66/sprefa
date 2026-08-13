export interface DfParam {
  path: string;
  node: string;
  pos: number;
  end: number;
}

export interface FlowNodeType {
  node: string;
  path: string;
  name: string;
  ty: string;
}

export interface FlowParamType {
  path: string;
  name: string;
  pos: number;
  ty: string;
}

export interface Sig {
  path: string;
  owner_start: number;
  owner_end: number;
  slot: string;
  pos: number;
  ty: string;
}

export interface SinkCallee {
  path: string;
  name: string;
}

export interface TypeOwner {
  path: string;
  name: string;
  start: number;
  end: number;
}
