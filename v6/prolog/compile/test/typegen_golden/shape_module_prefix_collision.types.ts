export interface AlphaStoreTicket {
  id: number;
}

export interface M9BetaStoreTicket {
  id: number;
  label: string;
}

export interface Audit {
  id: number;
  subject: AlphaStoreTicket;
}
