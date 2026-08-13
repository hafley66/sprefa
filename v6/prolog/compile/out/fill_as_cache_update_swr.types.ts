export interface CacheRow {
  target: string;
  body: string;
}

export interface Demanded {
  target: string;
  session_id: string;
}

export interface FeedView {
  target: string;
  body: string;
}

export interface FillArrived {
  target: string;
  body: string;
}

export interface OpenFeed {
  session_id: string;
  target: string;
}
