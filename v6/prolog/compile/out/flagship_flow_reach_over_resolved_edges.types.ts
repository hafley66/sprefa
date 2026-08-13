export interface FlowEdge {
  from_path: string;
  from_name: string;
  to_path: string;
  to_name: string;
}

export interface FlowReach {
  from_path: string;
  from_name: string;
  to_path: string;
  to_name: string;
}

export interface ResolvedCallEdge {
  caller_path: string;
  caller_name: string;
  callee_path: string;
  callee_name: string;
}
