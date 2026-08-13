export interface ClosedInner {
  outer_id: string;
  inner_id: string;
}

export interface ClosedOuter {
  outer_id: string;
}

export interface EndASignal {
  outer_id: string;
}

export interface EndBSignal {
  outer_id: string;
}

export interface EndCSignal {
  inner_id: string;
}

export interface LiveInner {
  outer_id: string;
  inner_id: string;
}

export interface LiveOuter {
  outer_id: string;
}

export interface OpenInner {
  outer_id: string;
  inner_id: string;
}

export interface OpenOuter {
  outer_id: string;
}
