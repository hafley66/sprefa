export interface Bundle {
  session: string;
  ai_run: number;
  user_run: number;
  ai_text: string;
  user_text: string;
}

export interface Handled {
  session: string;
  user_run: number;
}

export interface LaterStartBetween {
  session: string;
  run_turn: number;
  turn_number: number;
}

export interface PrevSameRole {
  session: string;
  turn_number: number;
}

export interface Resident {
  session: string;
  user_run: number;
  col3: number;
  col4: string;
}

export interface ResidentAsk {
  session: string;
  user_run: number;
  prompt: string;
}

export interface Run {
  session: string;
  run_turn: number;
  role: string;
  ai_text: string;
}

export interface RunBetween {
  session: string;
  ai_run: number;
  user_run: number;
}

export interface RunMember {
  session: string;
  run_turn: number;
  turn_number: number;
}

export interface RunSaid {
  session: string;
  run_turn: number;
  role: string;
  turn_number: number;
  said: string;
}

export interface RunStart {
  session: string;
  turn_number: number;
  role: string;
}

export interface Turn {
  session: string;
  turn_number: number;
  col3: number;
  role: string;
  said: string;
}
