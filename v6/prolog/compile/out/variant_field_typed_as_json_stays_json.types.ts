export interface PayloadBlob {
  id: number;
  data: unknown;
}

export interface PayloadNone {
  id: number;
}

export interface PayloadTag {
  id: number;
  tag: string;
}
