export interface Flagged {
  orchard_id: number;
}

export interface Orchard {
  orchard_id: number;
}

export interface Flag {
  parent: Orchard;
}

export interface Planted {
  orchard_id: number;
  tree_id: number;
}
