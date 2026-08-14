export interface FetchResultError {
  endpoint: string;
  status: number;
}

export interface FetchResultFresh {
  endpoint: string;
  tag: string;
  body: string;
}

export interface FetchResultUnchanged {
  endpoint: string;
}

export interface RespRaw {
  endpoint: string;
  status: number;
  tag: string;
  body: string;
}
