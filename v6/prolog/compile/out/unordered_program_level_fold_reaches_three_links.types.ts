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
