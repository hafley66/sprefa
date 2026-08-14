export interface Demanded {
  target: string;
  pane_id: string;
}

export interface DetailRow {
  item_id: string;
  body: string;
}

export interface DetailView {
  item_id: string;
  body: string;
}

export interface LiveDetail {
  pane_id: string;
  target: string;
}

export interface OpenDetail {
  pane_id: string;
  item_id: string;
}

export interface OpenPane {
  col1: string;
  col2: string;
}
