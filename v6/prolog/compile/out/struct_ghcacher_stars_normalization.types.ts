export interface CurrentBody {
  ep: string;
  body: RepoBody;
}

export interface RepoBody {
  full_name: string;
  stargazers_count: number;
}

export interface Stars {
  ep: string;
  n: number;
}
