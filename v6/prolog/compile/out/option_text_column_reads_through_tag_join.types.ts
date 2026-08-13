export interface EmailState {
  user_id: number;
  state: string;
}

export interface UserProfile {
  user_id: number;
  email: string | null;
}
