export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export type Status =
  | { tag: 'ready' }
  | { tag: 'failed'; reason: string; }
;

export interface StatusReady {
  id: number;
}

export interface StatusFailed {
  id: number;
  reason: string;
}

export interface Job {
  id: number;
  state: Option<Status>;
  nested_state: Option<Option<Status>>;
}
