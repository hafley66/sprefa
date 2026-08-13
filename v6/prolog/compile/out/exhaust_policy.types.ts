export interface CloseRequest {
  session_id: string;
  tab_id: string;
}

export interface Closed {
  session_id: string;
  tab_id: string;
}

export interface Demanded {
  col1: string;
  session_id: string;
}

export interface LiveTab {
  session_id: string;
  tab_id: string;
}

export interface OpenRequest {
  session_id: string;
  tab_id: string;
}

export interface OpenTab {
  session_id: string;
  tab_id: string;
}

export interface TabRow {
  tab_id: string;
  body: string;
}

export interface TabView {
  tab_id: string;
  body: string;
}
