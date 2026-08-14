export interface DispatchLeg {
  leg_id: number;
  dispatch_id: number;
  previous_leg: number;
  kilos: number;
}

export interface LegTotal {
  leg_id: number;
  dispatch_id: number;
  kilos: number;
}

export interface Ping {
  partition: string;
}

export interface PingOrdinal {
  partition: string;
  col2: number;
}

export interface SeqPingOrdinal2 {
  partition: string;
  at: number;
}
