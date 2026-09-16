export interface UserRecord {
  id: string;
  name: string;
}

export interface TeamRecord {
  owner: UserRecord;
  label: string;
}
