export interface FighterSummary {
  name: string;
  url: string;
}

export interface Squad {
  id: number;
  members: Array<FighterSummary>;
}
