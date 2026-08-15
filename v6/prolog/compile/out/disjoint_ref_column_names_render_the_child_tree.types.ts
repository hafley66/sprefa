export interface Carrier {
  id: number;
  nested: ShellPair;
}

export interface LeafPair {
  left: number;
  right: number;
}

export interface Seen {
  id: number;
}

export interface ShellPair {
  head: LeafPair;
  tail: LeafPair;
}
