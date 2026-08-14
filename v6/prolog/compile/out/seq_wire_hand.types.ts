export interface Arrival {
  payload: string;
}

export interface Numbered {
  ordinal: number;
  payload: string;
}

export interface SeqNumbered1 {
  partition: string;
  at: number;
}
