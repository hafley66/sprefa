export interface Demanded {
  target: string;
  session_id: string;
}

export interface OpenScope {
  session_id: string;
  target: string;
}

export interface RouteChange {
  session_id: string;
  route_id: string;
}

export interface RouteRow {
  route_id: string;
  body: string;
}

export interface RouteView {
  route_id: string;
  body: string;
}
