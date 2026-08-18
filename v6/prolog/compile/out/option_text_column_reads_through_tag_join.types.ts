export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export interface EmailState {
  user_id: number;
  state: string;
}

export interface UserProfile {
  user_id: number;
  email: Option<string>;
}
