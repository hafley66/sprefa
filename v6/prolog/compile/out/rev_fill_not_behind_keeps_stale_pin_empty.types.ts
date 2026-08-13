export interface DemandRev {
  dep_repo_id: number;
  ref_text: string;
}

export interface PinWant {
  col1: number;
  dep_repo_id: number;
  ref_text: string;
}

export interface RevFill {
  dep_repo_id: number;
  ref_text: string;
  behind: number;
  ahead: number;
}

export interface RevStatus {
  dep_repo_id: number;
  ref_text: string;
  behind: number;
  ahead: number;
}

export interface StalePin {
  dep_repo_id: number;
  ref_text: string;
}
