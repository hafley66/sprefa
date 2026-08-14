export interface ReviewDone {
  id: number;
  verdict: string;
}

export interface ReviewPending {
  id: number;
}

export interface ReviewTag {
  id: number;
  tag: string;
}

export interface Submission {
  id: number;
  verdict: string;
}
