export interface Event {
  payload: unknown;
}

export interface StarEvent {
  repo: string;
  stars: number;
}

export interface Total {
  repo: string;
  sum: number;
}
