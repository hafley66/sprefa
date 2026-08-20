export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export interface Session {
  token: Option<string>;
  user_id: number;
}
