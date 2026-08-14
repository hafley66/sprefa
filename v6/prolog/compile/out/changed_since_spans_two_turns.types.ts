export interface AgentTurn {
  turn_id: string;
  tick: number;
}

export interface ChangeEvent {
  path: string;
  digest: string;
  tick: number;
}

export interface ChangedSince {
  turn_id: string;
  path: string;
}

export interface TurnMarker {
  turn_id: string;
}

export interface WorktreeEdit {
  path: string;
  digest: string;
}
