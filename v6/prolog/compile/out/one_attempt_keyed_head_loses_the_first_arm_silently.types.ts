export interface DispatchAck {
  dispatch_id: number;
}

export interface DispatchSeal {
  sealed_id: number;
}

export interface DispatchWinner {
  dispatch_id: number;
  col2: string;
}
