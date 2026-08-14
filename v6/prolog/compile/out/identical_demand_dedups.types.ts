export interface Demand {
  args: string;
  salt: string;
}

export interface Fill {
  args: string;
  salt: string;
  payload: string;
}

export interface Response {
  args: string;
  salt: string;
  payload: string;
}

export interface WatchRequest {
  col1: string;
  args: string;
  salt: string;
}
