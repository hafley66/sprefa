export interface DfArg {
  caller_path: string;
  call_start: number;
  call_end: number;
  pos: number;
  arg: string;
  arg_end: number;
}

export interface DfParam {
  callee_path: string;
  param: string;
  pos: number;
  param_end: number;
}

export interface FlowEdge {
  from: string;
  to: string;
}

export interface ResolvedCallEdge {
  caller_path: string;
  call_start: number;
  call_end: number;
  callee_path: string;
  callee_name: string;
}
