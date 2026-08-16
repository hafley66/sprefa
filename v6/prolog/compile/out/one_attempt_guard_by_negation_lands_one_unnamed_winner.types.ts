export interface DispatchAck {
  dispatch_id: number;
}

export interface DispatchFirst {
  dispatch_id: number;
  ack_tag: string;
}

export interface DispatchSeal {
  sealed_id: number;
}
